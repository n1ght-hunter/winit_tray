//! Windows popup window implementation.

use std::{cell::Cell, ptr, rc::Rc};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_USERDATA, GetCursorPos, HWND_TOPMOST, KillTimer, PostMessageW, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetTimer, SetWindowPos, ShowWindow, WA_INACTIVE, WM_ACTIVATE, WM_CREATE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_NCCREATE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP
    },
};
use dpi::{PhysicalPosition, PhysicalSize};
use winit_core::event::{ElementState, MouseButton};
use winit_tray_core::popup::{Popup as CorePopup, PopupAttributes, PopupCloseReason, PopupEvent, PopupId, PopupProxy};

use crate::msg::POPUP_CLOSE_MSG_ID;
use crate::util;
use crate::Runner;

/// Timer ID for auto-dismiss functionality.
const AUTO_DISMISS_TIMER_ID: usize = 1;

/// We need to pass the window handle to the event loop thread, which means it needs to be
/// Send+Sync.
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct SyncWindowHandle(HWND);

unsafe impl Send for SyncWindowHandle {}
unsafe impl Sync for SyncWindowHandle {}

impl SyncWindowHandle {
    fn hwnd(&self) -> HWND {
        self.0
    }
}

static POPUP_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(1);

/// A popup window on Windows.
pub struct Popup<T = ()> {
    window_handle: SyncWindowHandle,
    proxy: PopupProxy<T>,
    internal_id: usize,
}

impl<T> std::fmt::Debug for Popup<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Popup")
            .field("window_handle", &self.window_handle)
            .field("internal_id", &self.internal_id)
            .finish()
    }
}

impl<T: Clone + Send + Sync + 'static> Popup<T> {
    /// Create a new popup window.
    pub fn new(proxy: PopupProxy<T>, attr: PopupAttributes<T>) -> Result<Self, anyhow::Error> {
        unsafe { init_popup(proxy, attr) }
    }

    /// Get the Windows window handle.
    #[inline]
    pub fn hwnd(&self) -> HWND {
        self.window_handle.hwnd()
    }
}

impl<T: Clone + Send + Sync + 'static> CorePopup for Popup<T> {
    fn id(&self) -> PopupId {
        PopupId::from_raw(self.internal_id)
    }

    fn close(&self) {
        unsafe {
            PostMessageW(self.window_handle.hwnd(), POPUP_CLOSE_MSG_ID.get(), 0, 0);
        }
    }

    fn set_position(&self, position: PhysicalPosition<i32>) {
        unsafe {
            SetWindowPos(
                self.window_handle.hwnd(),
                HWND_TOPMOST,
                position.x,
                position.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    fn set_size(&self, size: PhysicalSize<u32>) {
        unsafe {
            SetWindowPos(
                self.window_handle.hwnd(),
                HWND_TOPMOST,
                0,
                0,
                size.width as i32,
                size.height as i32,
                SWP_NOMOVE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    fn set_visible(&self, visible: bool) {
        unsafe {
            if visible {
                ShowWindow(self.window_handle.hwnd(), SW_SHOWNOACTIVATE);
            } else {
                ShowWindow(self.window_handle.hwnd(), SW_HIDE); 
            }
        }
    }
}

impl<T> Drop for Popup<T> {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.window_handle.hwnd(), POPUP_CLOSE_MSG_ID.get(), 0, 0);
        }
    }
}

/// Type-erased event sender for popup events.
type ErasedPopupEventSender = Box<dyn Fn(PopupId, PopupEvent<()>) + Send + Sync>;

/// Type-erased menu handler for popup context menus.
#[cfg(feature = "menu")]
type ErasedMenuHandler = Box<dyn Fn(HWND, i32, i32) + Send + Sync>;

/// Data stored per popup window.
struct PopupWindowData {
    pub userdata_removed: Cell<bool>,
    pub recurse_depth: Cell<u32>,
    pub runner: Rc<Runner>,
    pub popup_id: PopupId,
    pub event_sender: ErasedPopupEventSender,
    pub close_on_click_outside: bool,
    #[cfg(feature = "menu")]
    pub menu_handler: Option<ErasedMenuHandler>,
}

impl PopupWindowData {
    pub fn send_event(&self, event: PopupEvent<()>) {
        (self.event_sender)(self.popup_id, event);
    }

    #[cfg(feature = "menu")]
    pub fn show_menu(&self, hwnd: HWND, x: i32, y: i32) {
        if let Some(ref handler) = self.menu_handler {
            handler(hwnd, x, y);
        }
    }
}

/// Initialization data for popup window creation.
#[repr(C)]
pub(crate) struct PopupInitData<T> {
    vtable: PopupInitDataVTable,
    pub attributes: PopupAttributes<T>,
    pub proxy: PopupProxy<T>,
    pub runner: Rc<Runner>,
    pub popup: Option<Popup<T>>,
}

/// Type-erased vtable for PopupInitData operations.
struct PopupInitDataVTable {
    on_nccreate: unsafe fn(*mut std::ffi::c_void, HWND) -> Option<isize>,
    on_create: unsafe fn(*mut std::ffi::c_void),
}

/// Header struct for accessing vtable from raw pointer.
#[repr(C)]
struct PopupInitDataHeader {
    vtable: PopupInitDataVTable,
}

unsafe fn popupinitdata_on_nccreate<T: Clone + Send + Sync + 'static>(
    this: *mut std::ffi::c_void,
    window: HWND,
) -> Option<isize> {
    unsafe {
        let initdata = &mut *(this as *mut PopupInitData<T>);
        initdata.on_nccreate(window)
    }
}

unsafe fn popupinitdata_on_create<T: Clone + Send + Sync + 'static>(this: *mut std::ffi::c_void) {
    unsafe {
        let initdata = &mut *(this as *mut PopupInitData<T>);
        initdata.on_create();
    }
}

impl<T: Clone + Send + Sync + 'static> PopupInitData<T> {
    fn new(attributes: PopupAttributes<T>, proxy: PopupProxy<T>, runner: Rc<Runner>) -> Self {
        Self {
            vtable: PopupInitDataVTable {
                on_nccreate: popupinitdata_on_nccreate::<T>,
                on_create: popupinitdata_on_create::<T>,
            },
            attributes,
            proxy,
            runner,
            popup: None,
        }
    }

    unsafe fn create_popup(&self, window: HWND) -> Popup<T> {
        let internal_id = POPUP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Popup {
            window_handle: SyncWindowHandle(window),
            proxy: self.proxy.clone(),
            internal_id,
        }
    }

    unsafe fn create_popup_data(&self, popup: &Popup<T>) -> PopupWindowData {
        let proxy = self.proxy.clone();
        let popup_id = popup.id();
        let close_on_click_outside = self.attributes.close_on_click_outside;

        // Create type-erased event sender for pointer and close events
        let event_sender: ErasedPopupEventSender = Box::new(move |id, event| {
            let typed_event = match event {
                PopupEvent::PointerButton {
                    state,
                    position,
                    button,
                } => PopupEvent::PointerButton {
                    state,
                    position,
                    button,
                },
                PopupEvent::Closed { reason } => PopupEvent::Closed { reason },
                #[cfg(feature = "menu")]
                PopupEvent::MenuItemClicked { .. } => return,
                // Handle non-exhaustive enum and __Phantom variant
                _ => return,
            };
            (proxy)(id, typed_event);
        });

        #[cfg(feature = "menu")]
        let menu_handler: Option<ErasedMenuHandler> =
            self.attributes.menu.as_ref().map(|items| {
                let items = items.clone();
                let proxy = self.proxy.clone();
                let popup_id = popup.id();
                let handler: ErasedMenuHandler = Box::new(move |hwnd, x, y| {
                    if let Some(id) =
                        unsafe { crate::menu::show_context_menu(hwnd, &items, x, y) }
                    {
                        (proxy)(popup_id, PopupEvent::MenuItemClicked { id });
                    }
                });
                handler
            });

        PopupWindowData {
            userdata_removed: Cell::new(false),
            recurse_depth: Cell::new(0),
            runner: self.runner.clone(),
            popup_id,
            event_sender,
            close_on_click_outside,
            #[cfg(feature = "menu")]
            menu_handler,
        }
    }

    unsafe fn on_nccreate(&mut self, window: HWND) -> Option<isize> {
        let res = self.runner.catch_unwind(|| {
            let popup = unsafe { self.create_popup(window) };
            let popup_data = unsafe { self.create_popup_data(&popup) };
            (popup, popup_data)
        });

        res.map(|(popup, popup_data)| {
            self.popup = Some(popup);
            let userdata = Box::into_raw(Box::new(popup_data));
            userdata as _
        })
    }

    pub unsafe fn on_create(&mut self) {
        let _popup = self.popup.as_mut().expect("failed popup creation");
    }
}

/// Initialize a popup window.
unsafe fn init_popup<T: Clone + Send + Sync + 'static>(
    proxy: PopupProxy<T>,
    attr: PopupAttributes<T>,
) -> Result<Popup<T>, anyhow::Error> {
    let class_name = util::encode_wide(&attr.class_name);

    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(popup_window_callback),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: util::get_instance_handle(),
        hIcon: ptr::null_mut(),
        hCursor: ptr::null_mut(),
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };

    // Register the window class (ignore errors for duplicate registration)
    unsafe { RegisterClassExW(&class) };

    let auto_dismiss_ms = attr.auto_dismiss_ms;
    let topmost = attr.topmost;
    let position = attr.position;
    let size = attr.size;

    let mut initdata = PopupInitData::new(attr, proxy, Default::default());

    // Extended styles for popup window
    let ex_style = WS_EX_TOOLWINDOW | if topmost { WS_EX_TOPMOST } else { 0 };

    let handle = unsafe {
        CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            ptr::null(),
            WS_POPUP,
            position.x,
            position.y,
            size.width as i32,
            size.height as i32,
            ptr::null_mut(),
            ptr::null_mut(),
            util::get_instance_handle(),
            &mut initdata as *mut _ as *mut _,
        )
    };

    // Resume any panic that occurred during window creation
    if let Err(panic_error) = initdata.runner.take_panic_error() {
        std::panic::resume_unwind(panic_error)
    }

    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    let popup = initdata.popup.unwrap();

    // Set up auto-dismiss timer if configured
    if let Some(ms) = auto_dismiss_ms {
        unsafe {
            SetTimer(handle, AUTO_DISMISS_TIMER_ID, ms, None);
        }
    }

    // Show the window without activating it
    unsafe {
        ShowWindow(handle, SW_SHOWNOACTIVATE);
    }

    Ok(popup)
}

/// Window callback for popup windows.
unsafe extern "system" fn popup_window_callback(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let userdata = unsafe { util::get_window_long(window, GWL_USERDATA) };

    let userdata_ptr = match (userdata, msg) {
        (0, WM_NCCREATE) => {
            let createstruct = unsafe { &mut *(lparam as *mut CREATESTRUCTW) };
            let initdata_ptr = createstruct.lpCreateParams as *mut PopupInitDataHeader;
            let vtable = unsafe { &(*initdata_ptr).vtable };

            let result =
                match unsafe { (vtable.on_nccreate)(createstruct.lpCreateParams, window) } {
                    Some(userdata) => unsafe {
                        util::set_window_long(window, GWL_USERDATA, userdata as _);
                        DefWindowProcW(window, msg, wparam, lparam)
                    },
                    None => -1,
                };

            return result;
        }
        (0, WM_CREATE) => return -1,
        (_, WM_CREATE) => unsafe {
            let createstruct = &mut *(lparam as *mut CREATESTRUCTW);
            let initdata_ptr = createstruct.lpCreateParams as *mut PopupInitDataHeader;
            let vtable = &(*initdata_ptr).vtable;
            (vtable.on_create)(createstruct.lpCreateParams);
            return DefWindowProcW(window, msg, wparam, lparam);
        },
        (0, _) => return unsafe { DefWindowProcW(window, msg, wparam, lparam) },
        _ => userdata as *mut PopupWindowData,
    };

    let (result, userdata_removed, recurse_depth) = {
        let userdata = unsafe { &*(userdata_ptr) };

        userdata.recurse_depth.set(userdata.recurse_depth.get() + 1);

        let result =
            unsafe { popup_window_callback_inner(window, msg, wparam, lparam, userdata) };

        let userdata_removed = userdata.userdata_removed.get();
        let recurse_depth = userdata.recurse_depth.get() - 1;
        userdata.recurse_depth.set(recurse_depth);

        (result, userdata_removed, recurse_depth)
    };

    if userdata_removed && recurse_depth == 0 {
        drop(unsafe { Box::from_raw(userdata_ptr) });
    }

    result
}

/// Inner callback handler for popup window messages.
unsafe fn popup_window_callback_inner(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    userdata: &PopupWindowData,
) -> LRESULT {
    let mut result = ProcResult::DefWindowProc(wparam);

    userdata
        .runner
        .catch_unwind(|| match msg {
            // Auto-dismiss timer expired
            WM_TIMER if wparam == AUTO_DISMISS_TIMER_ID => {
                unsafe { KillTimer(window, AUTO_DISMISS_TIMER_ID) };
                userdata.send_event(PopupEvent::Closed {
                    reason: PopupCloseReason::Timeout,
                });
                userdata.userdata_removed.set(true);
                unsafe { util::set_window_long(window, GWL_USERDATA, 0) };
                unsafe { DestroyWindow(window) };
                result = ProcResult::Value(0);
            }

            // Click outside detection - when window loses activation
            WM_ACTIVATE if (wparam & 0xFFFF) as u32 == WA_INACTIVE => {
                if userdata.close_on_click_outside {
                    userdata.send_event(PopupEvent::Closed {
                        reason: PopupCloseReason::ClickOutside,
                    });
                    userdata.userdata_removed.set(true);
                    unsafe { util::set_window_long(window, GWL_USERDATA, 0) };
                    unsafe { DestroyWindow(window) };
                    result = ProcResult::Value(0);
                }
            }

            // Mouse button events
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN
            | WM_MBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let mut point = POINT { x: 0, y: 0 };
                if unsafe { GetCursorPos(&mut point) } != 0 {
                    let position = PhysicalPosition::new(point.x as f64, point.y as f64);

                    let (state, button) = match msg {
                        WM_LBUTTONDOWN => (ElementState::Pressed, MouseButton::Left),
                        WM_LBUTTONUP => (ElementState::Released, MouseButton::Left),
                        WM_RBUTTONDOWN => (ElementState::Pressed, MouseButton::Right),
                        WM_RBUTTONUP => (ElementState::Released, MouseButton::Right),
                        WM_MBUTTONDOWN => (ElementState::Pressed, MouseButton::Middle),
                        WM_MBUTTONUP => (ElementState::Released, MouseButton::Middle),
                        WM_XBUTTONDOWN => {
                            let xbutton = ((wparam >> 16) & 0xFFFF) as u16;
                            if xbutton == 1 {
                                (ElementState::Pressed, MouseButton::Back)
                            } else {
                                (ElementState::Pressed, MouseButton::Forward)
                            }
                        }
                        WM_XBUTTONUP => {
                            let xbutton = ((wparam >> 16) & 0xFFFF) as u16;
                            if xbutton == 1 {
                                (ElementState::Released, MouseButton::Back)
                            } else {
                                (ElementState::Released, MouseButton::Forward)
                            }
                        }
                        _ => unreachable!(),
                    };

                    userdata.send_event(PopupEvent::PointerButton {
                        state,
                        position,
                        button: winit_core::event::ButtonSource::Mouse(button),
                    });

                    // Show context menu on right-click release
                    #[cfg(feature = "menu")]
                    if msg == WM_RBUTTONUP {
                        userdata.show_menu(window, point.x, point.y);
                    }
                }
                result = ProcResult::Value(0);
            }

            _ => {
                // Handle close message
                if msg == POPUP_CLOSE_MSG_ID.get() {
                    userdata.send_event(PopupEvent::Closed {
                        reason: PopupCloseReason::Explicit,
                    });
                    userdata.userdata_removed.set(true);
                    unsafe { util::set_window_long(window, GWL_USERDATA, 0) };
                    unsafe { DestroyWindow(window) };
                    result = ProcResult::Value(0);
                } else {
                    result = ProcResult::DefWindowProc(wparam);
                }
            }
        })
        .unwrap_or_else(|| result = ProcResult::Value(-1));

    match result {
        ProcResult::DefWindowProc(wparam) => unsafe {
            DefWindowProcW(window, msg, wparam, lparam)
        },
        ProcResult::Value(val) => val,
    }
}

/// Result of window procedure processing.
#[derive(Clone, Copy)]
pub(crate) enum ProcResult {
    DefWindowProc(WPARAM),
    Value(isize),
}
