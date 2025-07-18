mod util;

use std::{cell::Cell, ptr};

use anyhow::Context;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassExW, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWL_USERDATA, WM_CREATE, WM_NCCREATE, WNDCLASSEXW, WS_OVERLAPPEDWINDOW
    },
};
use winit::raw_window_handle::RawWindowHandle;
use winit_tray_core::{Tray as CoreTray, TrayAttributes, TrayEvent, TrayProxy};

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

pub struct Tray {
    window_handle: SyncWindowHandle,
    proxy: TrayProxy,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("window_handle", &self.window_handle)
            .field("proxy", &"<...>") // Proxy is a closure, so we can't display it directly
            .finish()
    }
}

impl Tray {
    pub fn new(
        proxy: TrayProxy,
        attr: winit_tray_core::TrayAttributes,
    ) -> Result<Self, anyhow::Error> {
        unsafe { init(proxy, attr) }
    }

    pub fn hwnd(&self) -> HWND {
        self.window_handle.hwnd()
    }
}

impl CoreTray for Tray {
    fn id(&self) -> winit_tray_core::tray_id::TrayId {
        // Generate a unique ID for the tray icon.
        // This is a placeholder; actual implementation may vary.
        winit_tray_core::tray_id::TrayId::from_raw(self.window_handle.hwnd() as usize)
    }
}

pub struct InitData {
    pub attributes: TrayAttributes,
    pub proxy: TrayProxy,
    pub panic_error: Cell<Option<Box<dyn std::any::Any + Send + 'static>>>,
    // outputs
    pub tray: Option<Tray>,
}

struct WindowData {
    pub userdata_removed: Cell<bool>,
    pub recurse_depth: Cell<u32>,
}

impl InitData {
    unsafe fn create_window(&self, window: HWND) -> Tray {
        Tray {
            window_handle: SyncWindowHandle(window),
            proxy: self.proxy.clone(),
        }
    }

    unsafe fn create_window_data(&self, tray: &Tray) -> WindowData {
        WindowData {
            userdata_removed: Cell::new(false),
            recurse_depth: Cell::new(0),
        }
    }

    unsafe fn on_nccreate(&mut self, window: HWND) -> Option<isize> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let window = unsafe { self.create_window(window) };
            let window_data = unsafe { self.create_window_data(&window) };
            (window, window_data)
        }));

        match result {
            Ok((win, userdata)) => {
                self.tray = Some(win);
                let userdata = Box::into_raw(Box::new(userdata));
                Some(userdata as _)
            }
            Err(err) => {
                self.panic_error.set(Some(err));
                None
            }
        }
    }
}
unsafe fn init(
    proxy: TrayProxy,
    attr: winit_tray_core::TrayAttributes,
) -> Result<Tray, anyhow::Error> {
    let title = util::encode_wide(&attr.tooltip);
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

    let parent_hwnd = match attr.parent_window() {
        Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get() as HWND),
        Some(_) => unreachable!("Invalid raw window handle type for parent window"),
        _ => None,
    };

    let mut initdata = InitData {
        tray: None,
        attributes: attr,
        panic_error: Cell::new(None),
        proxy,
    };

    let handle = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            parent_hwnd.unwrap_or(ptr::null_mut()),
            ptr::null_mut(),
            util::get_instance_handle(),
            &mut initdata as *mut _ as *mut _,
        )
    };

    // If the window creation in `InitData` panicked, then should resume panicking here
    if let Some(panic_error) = initdata.panic_error.take() {
        std::panic::resume_unwind(panic_error)
    }

    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    let tray = initdata.tray.unwrap();

    Ok(tray)
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
            let initdata = unsafe { &mut *(createstruct.lpCreateParams as *mut InitData<'_>) };

            let result = match unsafe { initdata.on_nccreate(window) } {
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
            let initdata = createstruct.lpCreateParams;
            let initdata = &mut *(initdata as *mut InitData);

            initdata.on_create();
            return DefWindowProcW(window, msg, wparam, lparam);
        },
        (0, _) => return unsafe { DefWindowProcW(window, msg, wparam, lparam) },
        _ => userdata as *mut WindowData,
    };

    return 0;
}
