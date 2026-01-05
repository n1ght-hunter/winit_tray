#![cfg(target_os = "windows")]

pub mod msg;
mod util;

#[cfg(feature = "menu")]
pub mod menu;

#[cfg(feature = "context_menu")]
pub mod context_menu;

use std::{cell::Cell, ffi::OsStr, ptr, rc::Rc};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, TRUE, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateIcon, CreateWindowExW,
            DefWindowProcW, DestroyWindow, GWL_USERDATA, GetCursorPos, HICON, IDI_APPLICATION,
            LoadIconW, PostMessageW, RegisterClassExW, WM_CREATE, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_NCCREATE, WM_RBUTTONDOWN, WM_RBUTTONUP,
            WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_OVERLAPPED,
        },
    },
};
use dpi::PhysicalPosition;
use rwh_06::RawWindowHandle;
use winit_core::{
    event::{ElementState, MouseButton},
    icon::{Icon, RgbaIcon},
};
use tracing::trace;
use winit_tray_core::{Tray as CoreTray, TrayAttributes, TrayEvent, TrayProxy};

use crate::msg::DESTROY_MSG_ID;

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
/// We need to pass the window handle to the event loop thread, which means it needs to be
/// Send+Sync.
struct SyncWindowHandle(HWND);

unsafe impl Send for SyncWindowHandle {}
unsafe impl Sync for SyncWindowHandle {}

impl SyncWindowHandle {
    fn hwnd(&self) -> HWND {
        self.0
    }
}

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub struct Tray<T = ()> {
    window_handle: SyncWindowHandle,
    proxy: TrayProxy<T>,
    inernal_id: u32,
}

impl<T> std::fmt::Debug for Tray<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("window_handle", &self.window_handle)
            .field("proxy", &"<...>") // Proxy is a closure, so we can't display it directly
            .finish()
    }
}

impl<T: Clone + Send + Sync + 'static> Tray<T> {
    pub fn new(
        proxy: TrayProxy<T>,
        attr: winit_tray_core::TrayAttributes<T>,
    ) -> Result<Self, anyhow::Error> {
        unsafe { init(proxy, attr) }
    }

    #[inline]
    pub fn hwnd(&self) -> HWND {
        self.window_handle.hwnd()
    }

    pub fn set_tooltip<S: AsRef<OsStr>>(&self, tooltip: Option<S>) -> Result<(), anyhow::Error> {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            uFlags: NIF_TIP,
            hWnd: self.hwnd(),
            uID: 1,
            // uID: self.internal_id,
            ..unsafe { std::mem::zeroed() }
        };
        if let Some(tooltip) = &tooltip {
            let tip = util::encode_wide(tooltip);
            #[allow(clippy::manual_memcpy)]
            for i in 0..tip.len().min(128) {
                nid.szTip[i] = tip[i];
            }
        }

        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &mut nid as _) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

impl<T> CoreTray for Tray<T> {
    fn id(&self) -> winit_tray_core::tray_id::TrayId {
        // Generate a unique ID for the tray icon.
        // This is a placeholder; actual implementation may vary.
        winit_tray_core::tray_id::TrayId::from_raw(self.window_handle.hwnd() as usize)
    }
}

impl<T> Drop for Tray<T> {
    fn drop(&mut self) {
        unsafe {
            // The window must be destroyed from the same thread that created it, so we send a
            // custom message to be handled by our callback to do the actual work.
            PostMessageW(self.window_handle.hwnd(), DESTROY_MSG_ID.get(), 0, 0);
        }
    }
}

/// Pixel structure for RGBA to BGRA conversion
#[repr(C)]
struct Pixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Pixel {
    fn convert_to_bgra(&mut self) {
        std::mem::swap(&mut self.r, &mut self.b);
    }
}

const PIXEL_SIZE: usize = std::mem::size_of::<Pixel>();

/// Converts an `Icon` to a Windows `HICON` handle.
/// Returns `None` if the icon cannot be converted.
pub(crate) fn icon_to_hicon(icon: &Icon) -> Option<HICON> {
    // Try to downcast to RgbaIcon
    if let Some(rgba) = icon.0.cast_ref::<RgbaIcon>() {
        let pixel_count = rgba.buffer().len() / PIXEL_SIZE;
        let mut and_mask = Vec::with_capacity(pixel_count);

        // We need to copy and convert the buffer since we can't mutate the original
        let mut bgra_buffer = rgba.buffer().to_vec();
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(bgra_buffer.as_mut_ptr() as *mut Pixel, pixel_count)
        };

        for pixel in pixels {
            and_mask.push(pixel.a.wrapping_sub(u8::MAX)); // invert alpha channel
            pixel.convert_to_bgra();
        }

        let handle = unsafe {
            CreateIcon(
                ptr::null_mut(),
                rgba.width() as i32,
                rgba.height() as i32,
                1,
                (PIXEL_SIZE * 8) as u8,
                and_mask.as_ptr(),
                bgra_buffer.as_ptr(),
            )
        };

        if !handle.is_null() {
            return Some(handle);
        }
    }

    None
}

#[repr(C)]
pub(crate) struct InitData<T> {
    // MUST be first field to match InitDataHeader layout
    vtable: InitDataVTable,
    pub attributes: TrayAttributes<T>,
    pub proxy: TrayProxy<T>,
    pub runner: Rc<Runner>,
    // outputs
    pub tray: Option<Tray<T>>,
}

#[derive(Default)]
pub(crate) struct Runner {
    pub panic_error: Cell<Option<Box<dyn std::any::Any + Send + 'static>>>,
}

impl Runner {
    pub fn catch_unwind<R>(&self, f: impl FnOnce() -> R) -> Option<R> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(result) => Some(result),
            Err(err) => {
                self.panic_error.set(Some(err));
                None
            }
        }
    }

    pub fn take_panic_error(&self) -> Result<(), Box<dyn std::any::Any + Send + 'static>> {
        match self.panic_error.take() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

/// Type-erased event sender that can send TrayEvent<T> for any T.
type ErasedEventSender = Box<dyn Fn(HWND, TrayEvent<()>) + Send + Sync>;

/// Type-erased menu handler that shows the menu and sends the MenuItemClicked event.
#[cfg(feature = "menu")]
type ErasedMenuHandler = Box<dyn Fn(HWND, i32, i32) + Send + Sync>;

struct WindowData {
    pub userdata_removed: Cell<bool>,
    pub recurse_depth: Cell<u32>,
    pub runner: Rc<Runner>,
    pub event_sender: ErasedEventSender,
    #[cfg(feature = "menu")]
    pub menu_handler: Option<ErasedMenuHandler>,
}

impl WindowData {
    pub fn send_pointer_event(&self, hwnd: HWND, event: TrayEvent<()>) {
        trace!(?event, "sending tray event");
        (self.event_sender)(hwnd, event);
    }

    #[cfg(feature = "menu")]
    pub fn show_menu(&self, hwnd: HWND, x: i32, y: i32) {
        if let Some(ref handler) = self.menu_handler {
            handler(hwnd, x, y);
        }
    }
}

/// Type-erased vtable functions for InitData<T>
unsafe fn initdata_on_nccreate<T: Clone + Send + Sync + 'static>(
    this: *mut std::ffi::c_void,
    window: HWND,
) -> Option<isize> {
    unsafe {
        let initdata = &mut *(this as *mut InitData<T>);
        initdata.on_nccreate(window)
    }
}

unsafe fn initdata_on_create<T: Clone + Send + Sync + 'static>(this: *mut std::ffi::c_void) {
    unsafe {
        let initdata = &mut *(this as *mut InitData<T>);
        initdata.on_create();
    }
}

impl<T: Clone + Send + Sync + 'static> InitData<T> {
    fn new(attributes: TrayAttributes<T>, proxy: TrayProxy<T>, runner: Rc<Runner>) -> Self {
        Self {
            vtable: InitDataVTable {
                on_nccreate: initdata_on_nccreate::<T>,
                on_create: initdata_on_create::<T>,
            },
            attributes,
            proxy,
            runner,
            tray: None,
        }
    }

    unsafe fn create_tray(&self, window: HWND) -> Tray<T> {
        Tray {
            window_handle: SyncWindowHandle(window),
            proxy: self.proxy.clone(),
            inernal_id: COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    unsafe fn create_tray_data(&self, _tray: &Tray<T>) -> WindowData {
        let proxy = self.proxy.clone();

        // Create a type-erased event sender for pointer events
        let event_sender: ErasedEventSender = Box::new(move |hwnd, event| {
            // Convert the unit event to the appropriate T event
            let typed_event = match event {
                TrayEvent::PointerButton { state, position, button } => {
                    TrayEvent::PointerButton { state, position, button }
                }
                #[cfg(feature = "menu")]
                TrayEvent::MenuItemClicked { .. } => {
                    // This shouldn't happen for pointer events
                    return;
                }
                _ => return, // Handle any future TrayEvent variants
            };
            (proxy)(
                winit_tray_core::tray_id::TrayId::from_raw(hwnd as usize),
                typed_event,
            );
        });

        #[cfg(feature = "menu")]
        let menu_handler: Option<ErasedMenuHandler> = self.attributes.menu.as_ref().map(|items| {
            let items = items.clone();
            let proxy = self.proxy.clone();
            let handler: ErasedMenuHandler = Box::new(move |hwnd, x, y| {
                if let Some(id) = unsafe { menu::show_context_menu(hwnd, &items, x, y) } {
                    (proxy)(
                        winit_tray_core::tray_id::TrayId::from_raw(hwnd as usize),
                        TrayEvent::MenuItemClicked { id },
                    );
                }
            });
            handler
        });

        WindowData {
            userdata_removed: Cell::new(false),
            recurse_depth: Cell::new(0),
            runner: self.runner.clone(),
            event_sender,
            #[cfg(feature = "menu")]
            menu_handler,
        }
    }

    unsafe fn on_nccreate(&mut self, window: HWND) -> Option<isize> {
        let res = self.runner.catch_unwind(|| {
            let tray = unsafe { self.create_tray(window) };
            let tray_data = unsafe { self.create_tray_data(&tray) };
            (tray, tray_data)
        });

        res.map(|(tray, tray_data)| {
            self.tray = Some(tray);
            let userdata = Box::into_raw(Box::new(tray_data));
            userdata as _
        })
    }

    pub unsafe fn on_create(&mut self) {
        let _tray = self.tray.as_mut().expect("failed window creation");
        // This is where you can perform additional setup after the window has been created.
    }
}
unsafe fn init<T: Clone + Send + Sync + 'static>(
    proxy: TrayProxy<T>,
    attr: winit_tray_core::TrayAttributes<T>,
) -> Result<Tray<T>, anyhow::Error> {
    let class_name = util::encode_wide(&attr.class_name);

    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(public_window_callback),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: util::get_instance_handle(),
        hIcon: ptr::null_mut(),
        hCursor: ptr::null_mut(), // must be null in order for cursor state to work properly
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };

    // We ignore errors because registering the same window class twice would trigger
    //  an error, and because errors here are detected during CreateWindowEx anyway.
    // Also since there is no weird element in the struct, there is no reason for this
    //  call to fail.
    unsafe { RegisterClassExW(&class) };

    let parent_hwnd = match attr.parent_window {
        Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get() as HWND),
        Some(_) => unreachable!("Invalid raw window handle type for parent window"),
        _ => None,
    };

    let mut initdata = InitData::new(attr, proxy, Default::default());

    let handle = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED |
            // WS_EX_TOOLWINDOW prevents this window from ever showing up in the taskbar, which
            // we want to avoid. If you remove this style, this window won't show up in the
            // taskbar *initially*, but it can show up at some later point. This can sometimes
            // happen on its own after several hours have passed, although this has proven
            // difficult to reproduce. Alternatively, it can be manually triggered by killing
            // `explorer.exe` and then starting the process back up.
            // It is unclear why the bug is triggered by waiting for several hours.
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            ptr::null(),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            0,
            CW_USEDEFAULT,
            0,
            parent_hwnd.unwrap_or(ptr::null_mut()),
            ptr::null_mut(),
            util::get_instance_handle(),
            &mut initdata as *mut _ as *mut _,
        )
    };

    // If the window creation in `InitData` panicked, then should resume panicking here
    if let Err(panic_error) = initdata.runner.take_panic_error() {
        std::panic::resume_unwind(panic_error)
    }

    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    let tray = initdata.tray.unwrap();

    // Convert the Icon to HICON if provided
    let hicon = initdata.attributes.icon.as_ref().and_then(icon_to_hicon);

    if !unsafe {
        register_tray_icon(
            tray.hwnd(),
            tray.inernal_id,
            hicon,
            (&initdata.attributes.tooltip).as_ref(),
        )
    } {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(tray)
}

/// Type-erased vtable for InitData operations needed during window creation.
struct InitDataVTable {
    on_nccreate: unsafe fn(*mut std::ffi::c_void, HWND) -> Option<isize>,
    on_create: unsafe fn(*mut std::ffi::c_void),
}

/// Header struct that contains the vtable, placed at the start of InitData<T>.
#[repr(C)]
struct InitDataHeader {
    vtable: InitDataVTable,
}

unsafe extern "system" fn public_window_callback(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let userdata = unsafe { util::get_window_long(window, GWL_USERDATA) };

    let userdata_ptr = match (userdata, msg) {
        (0, WM_NCCREATE) => {
            let createstruct = unsafe { &mut *(lparam as *mut CREATESTRUCTW) };
            let initdata_ptr = createstruct.lpCreateParams as *mut InitDataHeader;
            let vtable = unsafe { &(*initdata_ptr).vtable };

            let result = match unsafe { (vtable.on_nccreate)(createstruct.lpCreateParams, window) } {
                Some(userdata) => unsafe {
                    util::set_window_long(window, GWL_USERDATA, userdata as _);
                    DefWindowProcW(window, msg, wparam, lparam)
                },
                None => -1, // failed to create the window
            };

            return result;
        }
        // Getting here should quite frankly be impossible,
        // but we'll make window creation fail here just in case.
        (0, WM_CREATE) => return -1,
        (_, WM_CREATE) => unsafe {
            let createstruct = &mut *(lparam as *mut CREATESTRUCTW);
            let initdata_ptr = createstruct.lpCreateParams as *mut InitDataHeader;
            let vtable = &(*initdata_ptr).vtable;
            (vtable.on_create)(createstruct.lpCreateParams);
            return DefWindowProcW(window, msg, wparam, lparam);
        },
        (0, _) => return unsafe { DefWindowProcW(window, msg, wparam, lparam) },
        _ => userdata as *mut WindowData,
    };

    let (result, userdata_removed, recurse_depth) = {
        let userdata = unsafe { &*(userdata_ptr) };

        userdata.recurse_depth.set(userdata.recurse_depth.get() + 1);

        let result = unsafe { public_window_callback_inner(window, msg, wparam, lparam, userdata) };

        let userdata_removed = userdata.userdata_removed.get();
        let recurse_depth = userdata.recurse_depth.get() - 1;
        userdata.recurse_depth.set(recurse_depth);

        (result, userdata_removed, recurse_depth)
    };

    if userdata_removed && recurse_depth == 0 {
        drop(unsafe { Box::from_raw(userdata_ptr) });
    }

    return result;
}

unsafe fn public_window_callback_inner(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    userdata: &WindowData,
) -> LRESULT {
    let mut result = ProcResult::DefWindowProc(wparam);

    userdata
        .runner
        .catch_unwind(|| match msg {
            WM_USER_TRAYICON if (lparam as u32 == WM_LBUTTONUP
                || lparam as u32 == WM_RBUTTONUP
                || lparam as u32 == WM_MBUTTONUP
                || lparam as u32 == WM_XBUTTONUP
                || lparam as u32 == WM_LBUTTONDOWN
                || lparam as u32 == WM_RBUTTONDOWN
                || lparam as u32 == WM_MBUTTONDOWN
                || lparam as u32 == WM_XBUTTONDOWN) =>
            {
                let mut point = POINT { x: 0, y: 0 };
                if unsafe { GetCursorPos(&mut point) } == 0 {
                    result = ProcResult::Value(-1);
                    return;
                }
                let position = PhysicalPosition::new(point.x as f64, point.y as f64);

                match lparam as u32 {
                    x if x == WM_LBUTTONUP => {
                        userdata.send_pointer_event(
                            window,
                            TrayEvent::PointerButton {
                                state: ElementState::Released,
                                position,
                                button: winit_core::event::ButtonSource::Mouse(MouseButton::Left),
                            },
                        );
                    }
                    x if x == WM_RBUTTONUP => {
                        userdata.send_pointer_event(
                            window,
                            TrayEvent::PointerButton {
                                state: ElementState::Released,
                                position,
                                button: winit_core::event::ButtonSource::Mouse(MouseButton::Right),
                            },
                        );

                        // Show context menu if configured
                        #[cfg(feature = "menu")]
                        userdata.show_menu(window, point.x, point.y);
                    }
                    x if x == WM_MBUTTONUP => {
                        userdata.send_pointer_event(
                            window,
                            TrayEvent::PointerButton {
                                state: ElementState::Released,
                                position,
                                button: winit_core::event::ButtonSource::Mouse(MouseButton::Middle),
                            },
                        );
                    }
                    x if x == WM_XBUTTONUP => {
                        if let Some(button) = MouseButton::try_from_u8(x as u8) {
                            userdata.send_pointer_event(
                                window,
                                TrayEvent::PointerButton {
                                    state: ElementState::Released,
                                    position,
                                    button: winit_core::event::ButtonSource::Mouse(button),
                                },
                            );
                        }
                    }
                    x if x == WM_LBUTTONDOWN => {
                        userdata.send_pointer_event(
                            window,
                            TrayEvent::PointerButton {
                                state: ElementState::Pressed,
                                position,
                                button: winit_core::event::ButtonSource::Mouse(MouseButton::Left),
                            },
                        );
                    }
                    x if x == WM_RBUTTONDOWN => {
                        userdata.send_pointer_event(
                            window,
                            TrayEvent::PointerButton {
                                state: ElementState::Pressed,
                                position,
                                button: winit_core::event::ButtonSource::Mouse(MouseButton::Right),
                            },
                        );
                    }
                    x if x == WM_MBUTTONDOWN => {
                        userdata.send_pointer_event(
                            window,
                            TrayEvent::PointerButton {
                                state: ElementState::Pressed,
                                position,
                                button: winit_core::event::ButtonSource::Mouse(MouseButton::Middle),
                            },
                        );
                    }
                    x if x == WM_XBUTTONDOWN => {
                        if let Some(button) = MouseButton::try_from_u8(x as u8) {
                            userdata.send_pointer_event(
                                window,
                                TrayEvent::PointerButton {
                                    state: ElementState::Pressed,
                                    position,
                                    button: winit_core::event::ButtonSource::Mouse(button),
                                },
                            );
                        }
                    }
                    _ => unreachable!("Invalid mouse button event"),
                };

                result = ProcResult::Value(0);
            }

            _ => {
                if msg == DESTROY_MSG_ID.get() {
                    unsafe { DestroyWindow(window) };
                    result = ProcResult::Value(0);
                } else {
                    result = ProcResult::DefWindowProc(wparam);
                }
            }
        })
        .unwrap_or_else(|| result = ProcResult::Value(-1));

    match result {
        ProcResult::DefWindowProc(wparam) => unsafe { DefWindowProcW(window, msg, wparam, lparam) },
        ProcResult::Value(val) => val,
    }
}

/// The result of a subclass procedure (the message handling callback)
#[derive(Clone, Copy)]
pub(crate) enum ProcResult {
    DefWindowProc(WPARAM),
    Value(isize),
}
const WM_USER_TRAYICON: u32 = 6002;

#[inline]
unsafe fn register_tray_icon<S: AsRef<OsStr>>(
    hwnd: HWND,
    tray_id: u32,
    hicon: Option<HICON>,
    tooltip: Option<S>,
) -> bool {
    let mut flags = NIF_MESSAGE | NIF_ICON;
    let mut sz_tip: [u16; 128] = [0; 128];

    let h_icon = if let Some(hicon) = hicon {
        hicon
    } else {
        let mut handle = unsafe {
            LoadIconW(
                GetModuleHandleW(std::ptr::null()),
                util::encode_wide("tray-default").as_ptr(),
            )
        };
        if handle.is_null() {
            // Fallback to a default icon if the specified icon could not be loaded
            handle = unsafe { LoadIconW(0 as _, IDI_APPLICATION) };
        }
        if handle.is_null() {
            return false;
        }
        handle
    };

    if let Some(tooltip) = tooltip {
        flags |= NIF_TIP;
        let tip = util::encode_wide(tooltip);
        #[allow(clippy::manual_memcpy)]
        for i in 0..tip.len().min(128) {
            sz_tip[i] = tip[i];
        }
    }

    let mut nid = NOTIFYICONDATAW {
        uFlags: flags,
        hWnd: hwnd,
        uID: tray_id,
        uCallbackMessage: WM_USER_TRAYICON,
        hIcon: h_icon,
        szTip: sz_tip,
        ..unsafe { std::mem::zeroed() }
    };

    unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid as _) == TRUE }
}
