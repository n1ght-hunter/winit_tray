use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt as _, ptr};

use windows_sys::Win32::{
    Foundation::{HMODULE, HWND},
    System::SystemServices::IMAGE_DOS_HEADER,
    UI::WindowsAndMessaging::{CreateIcon, HICON, WINDOW_LONG_PTR_INDEX},
};
use winit_core::icon::{Icon, RgbaIcon};

pub fn get_instance_handle() -> HMODULE {
    // Gets the instance handle by taking the address of the
    // pseudo-variable created by the microsoft linker:
    // https://devblogs.microsoft.com/oldnewthing/20041025-00/?p=37483

    // This is preferred over GetModuleHandle(NULL) because it also works in DLLs:
    // https://stackoverflow.com/questions/21718027/getmodulehandlenull-vs-hinstance

    unsafe extern "C" {
        static __ImageBase: IMAGE_DOS_HEADER;
    }

    unsafe { &__ImageBase as *const _ as _ }
}

#[inline(always)]
pub(crate) unsafe fn get_window_long(hwnd: HWND, nindex: WINDOW_LONG_PTR_INDEX) -> isize {
    #[cfg(target_pointer_width = "64")]
    return unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, nindex) };
    #[cfg(target_pointer_width = "32")]
    return unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongW(hwnd, nindex) as isize
    };
}

#[inline(always)]
pub(crate) unsafe fn set_window_long(
    hwnd: HWND,
    nindex: WINDOW_LONG_PTR_INDEX,
    dwnewlong: isize,
) -> isize {
    #[cfg(target_pointer_width = "64")]
    return unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, nindex, dwnewlong)
    };
    #[cfg(target_pointer_width = "32")]
    return unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongW(hwnd, nindex, dwnewlong as i32)
            as isize
    };
}

pub fn encode_wide(string: impl AsRef<OsStr>) -> Vec<u16> {
    string.as_ref().encode_wide().chain(once(0)).collect()
}

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

pub fn icon_to_hicon(icon: &Icon) -> Option<HICON> {
    if let Some(rgba) = icon.0.cast_ref::<RgbaIcon>() {
        let pixel_count = rgba.buffer().len() / PIXEL_SIZE;
        let mut and_mask = Vec::with_capacity(pixel_count);

        let mut bgra_buffer = rgba.buffer().to_vec();
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(bgra_buffer.as_mut_ptr() as *mut Pixel, pixel_count)
        };

        for pixel in pixels {
            and_mask.push(pixel.a.wrapping_sub(u8::MAX));
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
