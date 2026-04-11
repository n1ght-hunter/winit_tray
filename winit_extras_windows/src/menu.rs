//! Windows context menu implementation for tray icons and popup windows.

use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

use dpi::PhysicalPosition;
use rwh_06::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::{
        Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
        Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, ClientToScreen, CreateCompatibleDC,
            CreateDIBSection, DIB_RGB_COLORS, DeleteDC, GetDC, GetMonitorInfoW, HBITMAP,
            MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
        },
    },
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DI_NORMAL, DestroyMenu, DrawIconEx, DrawMenuBar,
        GetMenuItemCount, GetSubMenu, HMENU, MENUITEMINFOW, MF_CHECKED, MF_GRAYED, MF_POPUP,
        MF_SEPARATOR, MF_STRING, MIIM_BITMAP, PostMessageW, SetForegroundWindow, SetMenuItemInfoW,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON,
        TPM_TOPALIGN, TrackPopupMenu, WM_NULL,
    },
};
use winit_core::icon::Icon;
use winit_extras_core::{MenuEntry, MenuItem, Submenu};

use crate::util::encode_wide;

/// Dark mode preference for Windows context menus (Windows 10 1903+).
///
/// This controls how menus appear on Windows 10 version 1903 and later.
/// On older Windows versions, dark mode is not supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum DarkModePreference {
    /// Follow the system theme setting (automatic).
    /// Menus will be dark when Windows is in dark mode, light otherwise.
    /// This is the recommended setting for most applications.
    #[default]
    System,
    /// Force dark mode regardless of system setting.
    ForceDark,
    /// Force light mode regardless of system setting.
    ForceLight,
}

impl DarkModePreference {
    fn to_u8(self) -> u8 {
        match self {
            DarkModePreference::System => 0,
            DarkModePreference::ForceDark => 1,
            DarkModePreference::ForceLight => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => DarkModePreference::ForceDark,
            2 => DarkModePreference::ForceLight,
            _ => DarkModePreference::System,
        }
    }

    /// Returns the SetPreferredAppMode value for this preference.
    fn to_app_mode(self) -> i32 {
        match self {
            DarkModePreference::System => 1,     // AllowDark - follows system
            DarkModePreference::ForceDark => 2,  // ForceDark
            DarkModePreference::ForceLight => 3, // ForceLight
        }
    }
}

static DARK_MODE_PREFERENCE: AtomicU8 = AtomicU8::new(0); // 0 = System (default)
static DARK_MODE_INITIALIZED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Initializes dark mode support for the application (Windows 10 1903+).
///
/// This should be called early in your application, ideally before creating
/// any windows or menus. It sets up the application to follow the system theme
/// by default.
///
/// If you don't call this function, [`set_dark_mode_preference`] will
/// automatically initialize dark mode when first called.
///
/// # Example
/// ```rust,no_run
/// use winit_extras_windows::menu::init_dark_mode;
///
/// fn main() {
///     // Initialize dark mode early
///     init_dark_mode();
///
///     // ... create windows and menus
/// }
/// ```
pub fn init_dark_mode() {
    if DARK_MODE_INITIALIZED.swap(true, Ordering::SeqCst) {
        return; // Already initialized
    }

    let uxtheme = encode_wide("uxtheme.dll");
    unsafe {
        let hmodule = LoadLibraryW(uxtheme.as_ptr());
        if hmodule.is_null() {
            return;
        }

        type ProcAddr = Option<unsafe extern "system" fn() -> isize>;

        // RefreshImmersiveColorPolicyState (ordinal 104) - refreshes the color policy
        type RefreshImmersiveColorPolicyStateFn = unsafe extern "system" fn();
        if let Some(func) = std::mem::transmute::<
            ProcAddr,
            Option<RefreshImmersiveColorPolicyStateFn>,
        >(GetProcAddress(hmodule, 104 as *const u8))
        {
            func();
        }

        // SetPreferredAppMode (ordinal 135) - set to AllowDark (1) by default
        type SetPreferredAppModeFn = unsafe extern "system" fn(i32) -> i32;
        if let Some(func) = std::mem::transmute::<ProcAddr, Option<SetPreferredAppModeFn>>(
            GetProcAddress(hmodule, 135 as *const u8),
        ) {
            func(1); // AllowDark - follows system theme
        }
    }
}

/// Returns `true` if the Windows system is currently in dark mode.
///
/// This checks the system-wide app theme setting, not individual app preferences.
/// Returns `false` on Windows versions prior to Windows 10 1809 or if detection fails.
///
/// # Example
/// ```rust,no_run
/// use winit_extras_windows::menu::is_system_dark_mode;
///
/// if is_system_dark_mode() {
///     println!("System is in dark mode");
/// }
/// ```
pub fn is_system_dark_mode() -> bool {
    let uxtheme = encode_wide("uxtheme.dll");
    unsafe {
        let hmodule = LoadLibraryW(uxtheme.as_ptr());
        if hmodule.is_null() {
            return false;
        }

        // ShouldAppsUseDarkMode (ordinal 132)
        type ShouldAppsUseDarkModeFn = unsafe extern "system" fn() -> i32;
        type ProcAddr = Option<unsafe extern "system" fn() -> isize>;

        if let Some(func) = std::mem::transmute::<ProcAddr, Option<ShouldAppsUseDarkModeFn>>(
            GetProcAddress(hmodule, 132 as *const u8),
        ) {
            return func() != 0;
        }
    }

    false
}

/// Sets the dark mode preference for context menus (Windows 10 1903+).
///
/// This affects the appearance of popup menus created by this library.
///
/// # Arguments
/// * `preference` - The dark mode preference to apply
///
/// # Example
/// ```rust,no_run
/// use winit_extras_windows::menu::{set_dark_mode_preference, DarkModePreference};
///
/// // Follow system theme (recommended)
/// set_dark_mode_preference(DarkModePreference::System);
///
/// // Force dark mode
/// set_dark_mode_preference(DarkModePreference::ForceDark);
/// ```
pub fn set_dark_mode_preference(preference: DarkModePreference) {
    // Auto-initialize if not already done
    if !DARK_MODE_INITIALIZED.load(Ordering::SeqCst) {
        init_dark_mode();
    }

    DARK_MODE_PREFERENCE.store(preference.to_u8(), Ordering::Relaxed);

    let uxtheme = encode_wide("uxtheme.dll");
    unsafe {
        let hmodule = LoadLibraryW(uxtheme.as_ptr());
        if hmodule.is_null() {
            return;
        }

        // SetPreferredAppMode (ordinal 135)
        type SetPreferredAppModeFn = unsafe extern "system" fn(i32) -> i32;
        type ProcAddr = Option<unsafe extern "system" fn() -> isize>;
        if let Some(func) = std::mem::transmute::<ProcAddr, Option<SetPreferredAppModeFn>>(
            GetProcAddress(hmodule, 135 as *const u8),
        ) {
            func(preference.to_app_mode());
        }

        // FlushMenuThemes (ordinal 136)
        type FlushMenuThemesFn = unsafe extern "system" fn();
        if let Some(func) = std::mem::transmute::<ProcAddr, Option<FlushMenuThemesFn>>(
            GetProcAddress(hmodule, 136 as *const u8),
        ) {
            func();
        }
    }
}

/// Returns the current dark mode preference setting.
///
/// This returns the preference that was set via [`set_dark_mode_preference`],
/// not the actual current appearance. To check if the system is in dark mode,
/// use [`is_system_dark_mode`].
pub fn get_dark_mode_preference() -> DarkModePreference {
    DarkModePreference::from_u8(DARK_MODE_PREFERENCE.load(Ordering::Relaxed))
}

/// Enables or disables dark mode for a specific window's menu bar (Windows 10 1903+).
///
/// This function uses both the documented `DwmSetWindowAttribute` API and the
/// undocumented `AllowDarkModeForWindow` API to enable dark mode for the window's
/// title bar and menu bar.
///
/// # Safety
/// The `hwnd` must be a valid window handle.
///
/// # Arguments
/// * `hwnd` - The window handle
/// * `enable` - `true` to enable dark mode, `false` for light mode
pub unsafe fn set_window_menu_dark_mode(hwnd: HWND, enable: bool) {
    unsafe {
        // Use the documented DWM API for title bar dark mode (Windows 10 20H1+, Windows 11)
        let dark_mode: u32 = if enable { 1 } else { 0 };
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark_mode as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // Also use the undocumented uxtheme APIs for menu bar
        let uxtheme = encode_wide("uxtheme.dll");
        let hmodule = LoadLibraryW(uxtheme.as_ptr());
        if !hmodule.is_null() {
            type ProcAddr = Option<unsafe extern "system" fn() -> isize>;

            // AllowDarkModeForWindow (ordinal 133)
            type AllowDarkModeForWindowFn = unsafe extern "system" fn(HWND, i32) -> i32;
            if let Some(func) = std::mem::transmute::<ProcAddr, Option<AllowDarkModeForWindowFn>>(
                GetProcAddress(hmodule, 133 as *const u8),
            ) {
                func(hwnd, if enable { 1 } else { 0 });
            }

            // FlushMenuThemes (ordinal 136) - refresh menu theme cache
            type FlushMenuThemesFn = unsafe extern "system" fn();
            if let Some(func) = std::mem::transmute::<ProcAddr, Option<FlushMenuThemesFn>>(
                GetProcAddress(hmodule, 136 as *const u8),
            ) {
                func();
            }
        }

        // Redraw the menu bar
        DrawMenuBar(hwnd);
    }
}

/// Enables or disables dark mode for a window's menu bar.
///
/// This is a safe wrapper around [`set_window_menu_dark_mode`] that works with
/// any window implementing `HasWindowHandle`.
///
/// Call this after changing themes to update the window's menu bar appearance
/// on Windows 10 1903+ and Windows 11.
///
/// # Example
/// ```rust,no_run
/// use winit_extras_windows::menu::{set_dark_mode_preference, set_window_menu_dark_mode_for_window, DarkModePreference};
///
/// // Change to dark mode
/// set_dark_mode_preference(DarkModePreference::ForceDark);
/// set_window_menu_dark_mode_for_window(&window, true);
/// ```
pub fn set_window_menu_dark_mode_for_window(window: &impl HasWindowHandle, enable: bool) {
    let Ok(handle) = window.window_handle() else {
        return;
    };

    if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
        let hwnd = win32_handle.hwnd.get() as HWND;
        unsafe { set_window_menu_dark_mode(hwnd, enable) };
    }
}

/// Forces a window's menu bar to redraw, updating its theme appearance.
///
/// On Windows 11, after changing the dark mode preference, you should call this
/// function on any windows that have a menu bar to ensure the menu bar updates
/// its appearance.
///
/// # Safety
/// The `hwnd` must be a valid window handle.
pub unsafe fn refresh_menu_bar(hwnd: HWND) {
    unsafe { DrawMenuBar(hwnd) };
}

/// Forces a window's menu bar to redraw, updating its theme appearance.
///
/// This is a safe wrapper around [`refresh_menu_bar`] that works with any
/// window implementing `HasWindowHandle`.
pub fn refresh_menu_bar_for_window(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else {
        return;
    };

    if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
        let hwnd = win32_handle.hwnd.get() as HWND;
        unsafe { DrawMenuBar(hwnd) };
    }
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
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };

    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

    if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        info.rcWork
    } else {
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
    let work_area = unsafe { get_work_area_for_point(x, y) };
    let (est_width, est_height) = estimate_menu_size(item_count);

    let would_overflow_right = x + est_width > work_area.right;
    let would_overflow_bottom = y + est_height > work_area.bottom;

    match (would_overflow_right, would_overflow_bottom) {
        (false, false) => MenuAlignment::BottomRight,
        (true, false) => MenuAlignment::BottomLeft,
        (false, true) => MenuAlignment::TopRight,
        (true, true) => MenuAlignment::TopLeft,
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
    unsafe { show_context_menu_with_alignment(hwnd, items, x, y, MenuAlignment::TopLeft) }
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
    let hmenu = unsafe { build_popup_menu(items, &mut id_map) };
    if hmenu.is_null() {
        return None;
    }

    fn count_items<T>(items: &[MenuEntry<T>]) -> usize {
        items
            .iter()
            .map(|item| match item {
                MenuEntry::Item(_) | MenuEntry::Separator => 1,
                MenuEntry::Submenu(sub) => 1 + count_items(&sub.items),
            })
            .sum()
    }

    let resolved_alignment = match alignment {
        MenuAlignment::Auto => unsafe { determine_smart_alignment(x, y, count_items(items)) },
        other => other,
    };

    let flags = match resolved_alignment {
        MenuAlignment::BottomRight => {
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD
        }
        MenuAlignment::TopLeft => {
            TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD
        }
        MenuAlignment::BottomLeft => {
            TPM_RIGHTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD
        }
        MenuAlignment::TopRight => {
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD
        }
        MenuAlignment::Auto => unreachable!(),
    };

    unsafe {
        SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(hmenu, flags, x, y, 0, hwnd, ptr::null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        destroy_menu_tree(hmenu);

        if selected > 0 {
            id_map.get(selected as u32)
        } else {
            None
        }
    }
}

unsafe fn build_popup_menu<T: Clone>(items: &[MenuEntry<T>], id_map: &mut IdMap<T>) -> HMENU {
    let hmenu = unsafe { CreatePopupMenu() };
    if hmenu.is_null() {
        return hmenu;
    }

    for item in items {
        match item {
            MenuEntry::Item(item) => unsafe { add_menu_item(hmenu, item, id_map) },
            MenuEntry::Submenu(submenu) => unsafe { add_submenu(hmenu, submenu, id_map) },
            MenuEntry::Separator => unsafe {
                AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            },
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
    unsafe { AppendMenuW(hmenu, flags, win_id as usize, label.as_ptr()) };

    if let Some(ref icon) = item.icon
        && let Some(hbitmap) = unsafe { icon_to_hbitmap(icon) }
    {
        unsafe { set_menu_item_bitmap(hmenu, win_id, hbitmap) };
    }
}

unsafe fn add_submenu<T: Clone>(hmenu: HMENU, submenu: &Submenu<T>, id_map: &mut IdMap<T>) {
    let child_hmenu = unsafe { build_popup_menu(&submenu.items, id_map) };
    if child_hmenu.is_null() {
        return;
    }

    let mut flags = MF_POPUP;
    if !submenu.enabled {
        flags |= MF_GRAYED;
    }

    let label = encode_wide(&submenu.label);
    unsafe { AppendMenuW(hmenu, flags, child_hmenu as usize, label.as_ptr()) };
}

unsafe fn set_menu_item_bitmap(hmenu: HMENU, id: u32, hbitmap: HBITMAP) {
    let mut info: MENUITEMINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
    info.fMask = MIIM_BITMAP;
    info.hbmpItem = hbitmap;
    unsafe { SetMenuItemInfoW(hmenu, id, 0, &info) };
}

unsafe fn destroy_menu_tree(hmenu: HMENU) {
    let count = unsafe { GetMenuItemCount(hmenu) };
    for i in 0..count {
        let submenu = unsafe { GetSubMenu(hmenu, i) };
        if !submenu.is_null() {
            unsafe { destroy_menu_tree(submenu) };
        }
    }
    unsafe { DestroyMenu(hmenu) };
}

unsafe fn icon_to_hbitmap(icon: &Icon) -> Option<HBITMAP> {
    const SIZE: i32 = 16;

    let hicon = crate::util::icon_to_hicon(icon)?;
    let hdc_screen = unsafe { GetDC(ptr::null_mut()) };
    if hdc_screen.is_null() {
        return None;
    }

    let hdc = unsafe { CreateCompatibleDC(hdc_screen) };
    if hdc.is_null() {
        unsafe { ReleaseDC(ptr::null_mut(), hdc_screen) };
        return None;
    }

    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = SIZE;
    bmi.bmiHeader.biHeight = -SIZE;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    let mut bits: *mut std::ffi::c_void = ptr::null_mut();
    let hbitmap =
        unsafe { CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0) };

    if hbitmap.is_null() {
        unsafe {
            DeleteDC(hdc);
            ReleaseDC(ptr::null_mut(), hdc_screen);
        };
        return None;
    }

    unsafe {
        let old_bitmap = SelectObject(hdc, hbitmap as _);
        DrawIconEx(hdc, 0, 0, hicon, SIZE, SIZE, 0, ptr::null_mut(), DI_NORMAL);
        SelectObject(hdc, old_bitmap);

        DeleteDC(hdc);
        ReleaseDC(ptr::null_mut(), hdc_screen)
    };

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
/// use winit_extras_windows::menu::show_context_menu_for_window;
/// use winit_extras_core::{MenuEntry, MenuItem};
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
                show_context_menu_with_alignment(
                    hwnd,
                    items,
                    screen_position.x,
                    screen_position.y,
                    MenuAlignment::Auto,
                )
            }
        }
        _ => None,
    }
}
