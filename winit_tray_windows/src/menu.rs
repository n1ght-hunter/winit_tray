//! Windows context menu implementation for tray icons.

#![allow(unsafe_op_in_unsafe_fn)]

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, GetDC, ReleaseDC, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    },
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, DrawIconEx, GetMenuItemCount, GetSubMenu,
        PostMessageW, SetForegroundWindow, SetMenuItemInfoW, TrackPopupMenu, DI_NORMAL, HMENU,
        MENUITEMINFOW, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MIIM_BITMAP,
        TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_RIGHTALIGN, WM_NULL,
    },
};
use winit_core::icon::Icon;
use winit_tray_core::{MenuEntry, MenuItem, Submenu};

use crate::util::encode_wide;

static DARK_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables or disables dark mode for context menus (Windows 10 1903+).
pub fn set_dark_mode(enabled: bool) {
    // TODO: This could probably be relaxed ordering
    DARK_MODE_ENABLED.store(enabled, Ordering::SeqCst);

    let uxtheme = encode_wide("uxtheme.dll");
    unsafe {
        let hmodule = LoadLibraryW(uxtheme.as_ptr());
        if hmodule.is_null() {
            return;
        }

        type SetPreferredAppModeFn = unsafe extern "system" fn(i32) -> i32;
        if let Some(func) = std::mem::transmute::<_, Option<SetPreferredAppModeFn>>(
            GetProcAddress(hmodule, 135 as *const u8),
        ) {
            func(if enabled { 2 } else { 3 });
        }

        type FlushMenuThemesFn = unsafe extern "system" fn();
        if let Some(func) = std::mem::transmute::<_, Option<FlushMenuThemesFn>>(
            GetProcAddress(hmodule, 136 as *const u8),
        ) {
            func();
        }
    }
}

pub fn is_dark_mode_enabled() -> bool {
    // TODO: This could probably be relaxed ordering
    DARK_MODE_ENABLED.load(Ordering::SeqCst)
}

/// Maps internal Windows menu IDs (u32) to user-provided IDs of type T.
struct IdMap<T> {
    ids: Vec<T>,
}

impl<T: Clone> IdMap<T> {
    fn new() -> Self {
        Self { ids: Vec::new() }
    }

    fn insert(&mut self, id: T) -> u32 {
        let index = self.ids.len() as u32 + 1; // Windows menu IDs start from 1
        self.ids.push(id);
        index
    }

    fn get(&self, index: u32) -> Option<T> {
        if index == 0 {
            return None;
        }
        self.ids.get((index - 1) as usize).cloned()
    }
}

/// # Safety
/// The `hwnd` must be a valid window handle.
pub unsafe fn show_context_menu<T: Clone>(
    hwnd: HWND,
    items: &[MenuEntry<T>],
    x: i32,
    y: i32,
) -> Option<T> {
    let mut id_map = IdMap::new();
    let hmenu = build_popup_menu(items, &mut id_map);
    if hmenu.is_null() {
        return None;
    }

    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        hmenu,
        TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        x,
        y,
        0,
        hwnd,
        ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    destroy_menu_tree(hmenu);

    if selected > 0 {
        id_map.get(selected as u32)
    } else {
        None
    }
}

unsafe fn build_popup_menu<T: Clone>(items: &[MenuEntry<T>], id_map: &mut IdMap<T>) -> HMENU {
    let hmenu = CreatePopupMenu();
    if hmenu.is_null() {
        return hmenu;
    }

    for item in items {
        match item {
            MenuEntry::Item(item) => add_menu_item(hmenu, item, id_map),
            MenuEntry::Submenu(submenu) => add_submenu(hmenu, submenu, id_map),
            MenuEntry::Separator => {
                AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            }
        }
    }

    hmenu
}

unsafe fn add_menu_item<T: Clone>(hmenu: HMENU, item: &MenuItem<T>, id_map: &mut IdMap<T>) {
    let mut flags = MF_STRING;
    if !item.enabled {
        flags |= MF_GRAYED;
    }
    if item.checked == Some(true) {
        flags |= MF_CHECKED;
    }

    let win_id = id_map.insert(item.id.clone());
    let label = encode_wide(&item.label);
    AppendMenuW(hmenu, flags, win_id as usize, label.as_ptr());

    if let Some(ref icon) = item.icon {
        if let Some(hbitmap) = icon_to_hbitmap(icon) {
            set_menu_item_bitmap(hmenu, win_id, hbitmap);
        }
    }
}

unsafe fn add_submenu<T: Clone>(hmenu: HMENU, submenu: &Submenu<T>, id_map: &mut IdMap<T>) {
    let child_hmenu = build_popup_menu(&submenu.items, id_map);
    if child_hmenu.is_null() {
        return;
    }

    let mut flags = MF_POPUP;
    if !submenu.enabled {
        flags |= MF_GRAYED;
    }

    let label = encode_wide(&submenu.label);
    AppendMenuW(hmenu, flags, child_hmenu as usize, label.as_ptr());
}

unsafe fn set_menu_item_bitmap(hmenu: HMENU, id: u32, hbitmap: HBITMAP) {
    let mut info: MENUITEMINFOW = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
    info.fMask = MIIM_BITMAP;
    info.hbmpItem = hbitmap;
    SetMenuItemInfoW(hmenu, id, 0, &info);
}

unsafe fn destroy_menu_tree(hmenu: HMENU) {
    let count = GetMenuItemCount(hmenu);
    for i in 0..count {
        let submenu = GetSubMenu(hmenu, i);
        if !submenu.is_null() {
            destroy_menu_tree(submenu);
        }
    }
    DestroyMenu(hmenu);
}

unsafe fn icon_to_hbitmap(icon: &Icon) -> Option<HBITMAP> {
    const SIZE: i32 = 16;

    let hicon = crate::icon_to_hicon(icon)?;
    let hdc_screen = GetDC(ptr::null_mut());
    if hdc_screen.is_null() {
        return None;
    }

    let hdc = CreateCompatibleDC(hdc_screen);
    if hdc.is_null() {
        ReleaseDC(ptr::null_mut(), hdc_screen);
        return None;
    }

    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = SIZE;
    bmi.bmiHeader.biHeight = -SIZE;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    let mut bits: *mut std::ffi::c_void = ptr::null_mut();
    let hbitmap = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);

    if hbitmap.is_null() {
        DeleteDC(hdc);
        ReleaseDC(ptr::null_mut(), hdc_screen);
        return None;
    }

    let old_bitmap = SelectObject(hdc, hbitmap as _);
    DrawIconEx(hdc, 0, 0, hicon, SIZE, SIZE, 0, ptr::null_mut(), DI_NORMAL);
    SelectObject(hdc, old_bitmap);

    DeleteDC(hdc);
    ReleaseDC(ptr::null_mut(), hdc_screen);

    Some(hbitmap)
}
