//! Windows context menu implementation for tray icons and popup windows.

#![allow(unsafe_op_in_unsafe_fn)]

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use dpi::PhysicalPosition;
use rwh_06::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, GetDC, GetMonitorInfoW,
        MonitorFromPoint, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    },
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, DrawIconEx, GetMenuItemCount, GetSubMenu,
        PostMessageW, SetForegroundWindow, SetMenuItemInfoW, TrackPopupMenu, DI_NORMAL, HMENU,
        MENUITEMINFOW, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MIIM_BITMAP,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_RIGHTALIGN,
        TPM_TOPALIGN, WM_NULL,
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

/// Menu alignment options for context menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuAlignment {
    /// Menu appears to the bottom-right of the position (top-left corner at position).
    /// This is the typical behavior for window context menus.
    #[default]
    BottomRight,
    /// Menu appears to the top-left of the position (bottom-right corner at position).
    /// This is typically used for tray icon menus.
    TopLeft,
    /// Menu appears to the bottom-left of the position (top-right corner at position).
    BottomLeft,
    /// Menu appears to the top-right of the position (bottom-left corner at position).
    TopRight,
    /// Automatically choose the best alignment based on screen position.
    /// The menu will flip to avoid going off-screen.
    Auto,
}

/// Get the work area (screen bounds excluding taskbar) for the monitor containing the given point.
unsafe fn get_work_area_for_point(x: i32, y: i32) -> RECT {
    let point = POINT { x, y };
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);

    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

    if GetMonitorInfoW(monitor, &mut info) != 0 {
        info.rcWork
    } else {
        // Fallback to a reasonable default if GetMonitorInfoW fails
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        }
    }
}

/// Estimate context menu size based on item count.
/// This is a rough estimate - actual size depends on text length, icons, etc.
fn estimate_menu_size(item_count: usize) -> (i32, i32) {
    // Typical menu item height is about 20-24 pixels on Windows
    // Width is harder to estimate, use a conservative default
    const ITEM_HEIGHT: i32 = 24;
    const EST_WIDTH: i32 = 300;

    let height = (item_count as i32 * ITEM_HEIGHT).max(ITEM_HEIGHT);
    (EST_WIDTH, height)
}

/// Determine the best menu alignment based on screen position.
///
/// This checks which edges of the screen the menu would overflow
/// and returns an alignment that will keep the menu on-screen.
unsafe fn determine_smart_alignment(x: i32, y: i32, item_count: usize) -> MenuAlignment {
    let work_area = get_work_area_for_point(x, y);
    let (est_width, est_height) = estimate_menu_size(item_count);

    // Check if menu would overflow right or bottom edges
    let would_overflow_right = x + est_width > work_area.right;
    let would_overflow_bottom = y + est_height > work_area.bottom;

    // Choose alignment based on which edges would overflow
    match (would_overflow_right, would_overflow_bottom) {
        (false, false) => MenuAlignment::BottomRight, // Normal case - menu goes down-right
        (true, false) => MenuAlignment::BottomLeft,   // Too close to right edge
        (false, true) => MenuAlignment::TopRight,     // Too close to bottom edge
        (true, true) => MenuAlignment::TopLeft,       // Corner case - menu goes up-left
    }
}

/// # Safety
/// The `hwnd` must be a valid window handle.
///
/// Shows a context menu with top-left alignment (menu appears above and to the left).
/// This is the default for tray icon menus.
pub unsafe fn show_context_menu<T: Clone>(
    hwnd: HWND,
    items: &[MenuEntry<T>],
    x: i32,
    y: i32,
) -> Option<T> {
    show_context_menu_with_alignment(hwnd, items, x, y, MenuAlignment::TopLeft)
}

/// # Safety
/// The `hwnd` must be a valid window handle.
///
/// Shows a context menu with the specified alignment.
pub unsafe fn show_context_menu_with_alignment<T: Clone>(
    hwnd: HWND,
    items: &[MenuEntry<T>],
    x: i32,
    y: i32,
    alignment: MenuAlignment,
) -> Option<T> {
    let mut id_map = IdMap::new();
    let hmenu = build_popup_menu(items, &mut id_map);
    if hmenu.is_null() {
        return None;
    }

    // Count items for smart alignment estimation
    fn count_items<T>(items: &[MenuEntry<T>]) -> usize {
        items
            .iter()
            .map(|item| match item {
                MenuEntry::Item(_) | MenuEntry::Separator => 1,
                MenuEntry::Submenu(sub) => 1 + count_items(&sub.items),
            })
            .sum()
    }

    // Resolve Auto alignment to a concrete alignment
    let resolved_alignment = match alignment {
        MenuAlignment::Auto => determine_smart_alignment(x, y, count_items(items)),
        other => other,
    };

    let flags = match resolved_alignment {
        MenuAlignment::BottomRight => TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        MenuAlignment::TopLeft => TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        MenuAlignment::BottomLeft => TPM_RIGHTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        MenuAlignment::TopRight => TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        MenuAlignment::Auto => unreachable!(), // Already resolved above
    };

    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        hmenu,
        flags,
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

/// Show a context menu for any window that implements `HasWindowHandle`.
///
/// This is a convenience wrapper around [`show_context_menu`] that handles
/// extracting the HWND from a winit window or any other window type.
///
/// The `position` should be in window-relative (client) coordinates.
/// This function will convert them to screen coordinates automatically.
///
/// Returns the selected menu item ID, or `None` if the menu was dismissed.
///
/// # Example
///
/// ```rust,no_run
/// use winit_tray_windows::menu::show_context_menu_for_window;
/// use winit_tray_core::{MenuEntry, MenuItem};
///
/// #[derive(Clone)]
/// enum Action { Open, Exit }
///
/// fn handle_right_click(window: &impl rwh_06::HasWindowHandle, x: i32, y: i32) {
///     let menu = vec![
///         MenuEntry::Item(MenuItem::new(Action::Open, "Open")),
///         MenuEntry::Separator,
///         MenuEntry::Item(MenuItem::new(Action::Exit, "Exit")),
///     ];
///
///     if let Some(action) = show_context_menu_for_window(window, &menu, (x, y).into()) {
///         match action {
///             Action::Open => println!("Open clicked"),
///             Action::Exit => println!("Exit clicked"),
///         }
///     }
/// }
/// ```
pub fn show_context_menu_for_window<T: Clone>(
    window: &impl HasWindowHandle,
    items: &[MenuEntry<T>],
    position: PhysicalPosition<i32>,
) -> Option<T> {
    let handle = window.window_handle().ok()?;

    match handle.as_raw() {
        RawWindowHandle::Win32(win32_handle) => {
            let hwnd = win32_handle.hwnd.get() as HWND;

            // Convert client coordinates to screen coordinates
            let mut point = POINT {
                x: position.x,
                y: position.y,
            };
            unsafe {
                ClientToScreen(hwnd, &mut point);
                // Use Auto alignment to smartly position menu based on screen bounds
                show_context_menu_with_alignment(hwnd, items, point.x, point.y, MenuAlignment::Auto)
            }
        }
        _ => None,
    }
}

/// Show a context menu at screen coordinates for any window that implements `HasWindowHandle`.
///
/// Similar to [`show_context_menu_for_window`], but the position is already in screen coordinates.
///
/// Returns the selected menu item ID, or `None` if the menu was dismissed.
pub fn show_context_menu_for_window_at_screen_pos<T: Clone>(
    window: &impl HasWindowHandle,
    items: &[MenuEntry<T>],
    screen_position: PhysicalPosition<i32>,
) -> Option<T> {
    let handle = window.window_handle().ok()?;

    match handle.as_raw() {
        RawWindowHandle::Win32(win32_handle) => {
            let hwnd = win32_handle.hwnd.get() as HWND;
            unsafe {
                // Use Auto alignment to smartly position menu based on screen bounds
                show_context_menu_with_alignment(hwnd, items, screen_position.x, screen_position.y, MenuAlignment::Auto)
            }
        }
        _ => None,
    }
}
