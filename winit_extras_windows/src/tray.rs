//! Tray icon implementation for Windows.

use std::{cell::Cell, ffi::OsStr, ptr, rc::Rc};

use dpi::PhysicalPosition;
use rwh_06::RawWindowHandle;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, TRUE, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
            DestroyWindow, GWL_USERDATA, GetCursorPos, HICON, IDI_APPLICATION, LoadIconW,
            PostMessageW, RegisterClassExW, WM_CREATE, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_NCCREATE, WM_RBUTTONDOWN, WM_RBUTTONUP,
            WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_OVERLAPPED,
        },
    },
};
use winit_core::event::{ElementState, MouseButton};
use winit_extras_core::{Event, EventCallback, TrayIcon as CoreTrayIcon, TrayIconAttributes};

use crate::msg::DESTROY_MSG_ID;
use crate::util;

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

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub struct Tray<T = ()> {
    window_handle: SyncWindowHandle,
    internal_id: u32,
    _marker: std::marker::PhantomData<T>,
}

impl<T> std::fmt::Debug for Tray<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("window_handle", &self.window_handle)
            .field("internal_id", &self.internal_id)
            .finish()
    }
}

impl<T: Clone + Send + Sync + 'static> Tray<T> {
    pub fn new(proxy: EventCallback<T>, attr: TrayIconAttributes) -> Result<Self, anyhow::Error> {
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
            uID: self.internal_id,
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

impl<T> CoreTrayIcon for Tray<T> {
    fn id(&self) -> winit_extras_core::tray_icon_id::TrayIconId {
        winit_extras_core::tray_icon_id::TrayIconId::from_raw(self.window_handle.hwnd() as usize)
    }
}

impl<T> Drop for Tray<T> {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.window_handle.hwnd(), DESTROY_MSG_ID.get(), 0, 0);
        }
    }
}

#[repr(C)]
pub(crate) struct InitData<T> {
    vtable: InitDataVTable,
    pub attributes: TrayIconAttributes,
    pub proxy: EventCallback<T>,
    pub runner: Rc<Runner>,
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

type ErasedEventSender = Box<
    dyn Fn(HWND, ElementState, PhysicalPosition<f64>, winit_core::event::ButtonSource)
        + Send
        + Sync,
>;

struct WindowData {
    pub userdata_removed: Cell<bool>,
    pub recurse_depth: Cell<u32>,
    pub runner: Rc<Runner>,
    pub event_sender: ErasedEventSender,
}

impl WindowData {
    pub fn send_pointer_event(
        &self,
        hwnd: HWND,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: winit_core::event::ButtonSource,
    ) {
        (self.event_sender)(hwnd, state, position, button);
    }
}

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
    fn new(attributes: TrayIconAttributes, proxy: EventCallback<T>, runner: Rc<Runner>) -> Self {
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
            internal_id: COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            _marker: std::marker::PhantomData,
        }
    }

    unsafe fn create_tray_data(&self, _tray: &Tray<T>) -> WindowData {
        let proxy = self.proxy.clone();

        let event_sender: ErasedEventSender = Box::new(move |hwnd, state, position, button| {
            let tray_icon_id = winit_extras_core::tray_icon_id::TrayIconId::from_raw(hwnd as usize);
            (proxy)(Event::PointerButton {
                tray_icon_id,
                state,
                position,
                button,
            });
        });

        WindowData {
            userdata_removed: Cell::new(false),
            recurse_depth: Cell::new(0),
            runner: self.runner.clone(),
            event_sender,
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
    }
}

unsafe fn init<T: Clone + Send + Sync + 'static>(
    proxy: EventCallback<T>,
    attr: TrayIconAttributes,
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
        hCursor: ptr::null_mut(),
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: ptr::null_mut(),
    };

    unsafe { RegisterClassExW(&class) };

    let parent_hwnd = match attr.parent_window {
        Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get() as HWND),
        Some(_) => unreachable!("Invalid raw window handle type for parent window"),
        _ => None,
    };

    let mut initdata = InitData::new(attr, proxy, Default::default());

    let handle = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
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

    if let Err(panic_error) = initdata.runner.take_panic_error() {
        std::panic::resume_unwind(panic_error)
    }

    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    let tray = initdata.tray.unwrap();

    let hicon = initdata
        .attributes
        .icon
        .as_ref()
        .and_then(util::icon_to_hicon);

    if !unsafe {
        register_tray_icon(
            tray.hwnd(),
            tray.internal_id,
            hicon,
            initdata.attributes.tooltip.as_ref(),
        )
    } {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(tray)
}

struct InitDataVTable {
    on_nccreate: unsafe fn(*mut std::ffi::c_void, HWND) -> Option<isize>,
    on_create: unsafe fn(*mut std::ffi::c_void),
}

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

            let result = match unsafe { (vtable.on_nccreate)(createstruct.lpCreateParams, window) }
            {
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

    result
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
            WM_USER_TRAYICON
                if (lparam as u32 == WM_LBUTTONUP
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

                let (state, button) = match lparam as u32 {
                    x if x == WM_LBUTTONUP => (ElementState::Released, MouseButton::Left),
                    x if x == WM_RBUTTONUP => (ElementState::Released, MouseButton::Right),
                    x if x == WM_MBUTTONUP => (ElementState::Released, MouseButton::Middle),
                    x if x == WM_LBUTTONDOWN => (ElementState::Pressed, MouseButton::Left),
                    x if x == WM_RBUTTONDOWN => (ElementState::Pressed, MouseButton::Right),
                    x if x == WM_MBUTTONDOWN => (ElementState::Pressed, MouseButton::Middle),
                    x if x == WM_XBUTTONUP => {
                        if let Some(button) = MouseButton::try_from_u8(x as u8) {
                            (ElementState::Released, button)
                        } else {
                            return;
                        }
                    }
                    x if x == WM_XBUTTONDOWN => {
                        if let Some(button) = MouseButton::try_from_u8(x as u8) {
                            (ElementState::Pressed, button)
                        } else {
                            return;
                        }
                    }
                    _ => unreachable!("Invalid mouse button event"),
                };

                userdata.send_pointer_event(
                    window,
                    state,
                    position,
                    winit_core::event::ButtonSource::Mouse(button),
                );

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

#[derive(Clone, Copy)]
pub(crate) enum ProcResult {
    DefWindowProc(WPARAM),
    Value(isize),
}

const WM_USER_TRAYICON: u32 = 6002;

#[inline]
unsafe fn register_tray_icon<S: AsRef<OsStr>>(
    hwnd: HWND,
    tray_icon_id: u32,
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
        uID: tray_icon_id,
        uCallbackMessage: WM_USER_TRAYICON,
        hIcon: h_icon,
        szTip: sz_tip,
        ..unsafe { std::mem::zeroed() }
    };

    unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid as _) == TRUE }
}
