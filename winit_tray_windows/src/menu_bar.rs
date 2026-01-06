//! Menu bar implementation for Windows.
//!
//! On Windows, the menu bar is attached to a window using SetMenu().

#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;
use std::ptr;
use std::sync::{Arc, Mutex};

use rwh_06::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreateMenu, CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetSubMenu,
        SetMenu, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    },
};
use winit_tray_core::menu_bar::{
    MenuBar as CoreMenuBar, MenuBarAttributes, MenuBarEvent, MenuBarId, MenuBarProxy,
    TopLevelMenu,
};
use winit_tray_core::{MenuEntry, MenuItem, Submenu};

use crate::util::encode_wide;

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

// Global storage for menu bar callbacks, keyed by HWND + command ID.
static MENU_BAR_CALLBACKS: std::sync::LazyLock<Mutex<HashMap<(usize, u32), Box<dyn Fn() + Send + Sync>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Maps internal Windows menu IDs (u32) to callbacks.
struct IdMap {
    next_id: u32,
}

impl IdMap {
    fn new() -> Self {
        Self { next_id: 1 } // Windows menu IDs start from 1
    }

    fn next(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Windows menu bar implementation.
pub struct MenuBar {
    internal_id: usize,
    hwnd: HWND,
    hmenu: HMENU,
}

impl std::fmt::Debug for MenuBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBar")
            .field("internal_id", &self.internal_id)
            .finish_non_exhaustive()
    }
}

impl MenuBar {
    /// Create a new menu bar with the given attributes.
    ///
    /// The `parent_window` attribute is required on Windows.
    pub fn new<T: Clone + Send + Sync + 'static>(
        proxy: MenuBarProxy<T>,
        attr: MenuBarAttributes<T>,
    ) -> Result<Self, anyhow::Error> {
        let hwnd = match attr.parent_window {
            Some(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as HWND,
            Some(_) => {
                return Err(anyhow::anyhow!(
                    "Invalid window handle type, expected Win32"
                ))
            }
            None => return Err(anyhow::anyhow!("parent_window is required on Windows")),
        };

        let internal_id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let menu_bar_id = MenuBarId::from_raw(internal_id);

        let mut id_map = IdMap::new();

        // Create the main menu bar
        let hmenu = unsafe { CreateMenu() };
        if hmenu.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        // Add top-level menus
        for top_level in &attr.menus {
            unsafe {
                add_top_level_menu(hmenu, top_level, &mut id_map, hwnd, proxy.clone(), menu_bar_id)?;
            }
        }

        // Attach the menu bar to the window
        if unsafe { SetMenu(hwnd, hmenu) } == 0 {
            unsafe { destroy_menu_tree(hmenu) };
            return Err(std::io::Error::last_os_error().into());
        }

        Ok(MenuBar {
            internal_id,
            hwnd,
            hmenu,
        })
    }

    /// Create a menu bar for a window that implements `HasWindowHandle`.
    pub fn new_for_window<T: Clone + Send + Sync + 'static>(
        window: &impl HasWindowHandle,
        proxy: MenuBarProxy<T>,
        menus: Vec<TopLevelMenu<T>>,
    ) -> Result<Self, anyhow::Error> {
        let handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Failed to get window handle: {}", e))?;

        let attr = MenuBarAttributes {
            menus,
            parent_window: Some(handle.as_raw()),
        };

        Self::new(proxy, attr)
    }
}

impl CoreMenuBar for MenuBar {
    fn id(&self) -> MenuBarId {
        MenuBarId::from_raw(self.internal_id)
    }

    fn remove(&self) {
        // Remove the menu bar from the window
        unsafe {
            SetMenu(self.hwnd, ptr::null_mut());
        }

        // Clean up callbacks for this window
        if let Ok(mut callbacks) = MENU_BAR_CALLBACKS.lock() {
            callbacks.retain(|(hwnd, _), _| *hwnd != self.hwnd as usize);
        }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        // Remove menu from window and destroy it
        unsafe {
            SetMenu(self.hwnd, ptr::null_mut());
            destroy_menu_tree(self.hmenu);
        }

        // Clean up callbacks for this window
        if let Ok(mut callbacks) = MENU_BAR_CALLBACKS.lock() {
            callbacks.retain(|(hwnd, _), _| *hwnd != self.hwnd as usize);
        }
    }
}

/// Handle WM_COMMAND messages for menu bar items.
///
/// Call this from your window procedure when you receive WM_COMMAND.
/// Returns true if the command was handled.
pub fn handle_menu_command(hwnd: HWND, command_id: u32) -> bool {
    if let Ok(callbacks) = MENU_BAR_CALLBACKS.lock() {
        if let Some(callback) = callbacks.get(&(hwnd as usize, command_id)) {
            callback();
            return true;
        }
    }
    false
}

/// Creates a top-level menu and adds it to the menu bar.
unsafe fn add_top_level_menu<T: Clone + Send + Sync + 'static>(
    hmenu_bar: HMENU,
    top_level: &TopLevelMenu<T>,
    id_map: &mut IdMap,
    hwnd: HWND,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<(), anyhow::Error> {
    let hmenu_popup = build_popup_menu(&top_level.items, id_map, hwnd, proxy, menu_bar_id)?;

    let label = encode_wide(&top_level.label);
    AppendMenuW(hmenu_bar, MF_POPUP, hmenu_popup as usize, label.as_ptr());

    Ok(())
}

/// Builds a popup menu from menu entries.
unsafe fn build_popup_menu<T: Clone + Send + Sync + 'static>(
    items: &[MenuEntry<T>],
    id_map: &mut IdMap,
    hwnd: HWND,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<HMENU, anyhow::Error> {
    let hmenu = CreatePopupMenu();
    if hmenu.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    for item in items {
        match item {
            MenuEntry::Item(item) => {
                add_menu_item(hmenu, item, id_map, hwnd, proxy.clone(), menu_bar_id);
            }
            MenuEntry::Submenu(submenu) => {
                add_submenu(hmenu, submenu, id_map, hwnd, proxy.clone(), menu_bar_id)?;
            }
            MenuEntry::Separator => {
                AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            }
        }
    }

    Ok(hmenu)
}

/// Adds a menu item to a menu.
unsafe fn add_menu_item<T: Clone + Send + Sync + 'static>(
    hmenu: HMENU,
    item: &MenuItem<T>,
    id_map: &mut IdMap,
    hwnd: HWND,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) {
    let mut flags = MF_STRING;
    if !item.enabled {
        flags |= MF_GRAYED;
    }
    if item.checked == Some(true) {
        flags |= MF_CHECKED;
    }

    let win_id = id_map.next();
    let label = encode_wide(&item.label);
    AppendMenuW(hmenu, flags, win_id as usize, label.as_ptr());

    // Store callback
    let id = item.id.clone();
    let callback: Box<dyn Fn() + Send + Sync> = Box::new(move || {
        proxy(menu_bar_id, MenuBarEvent::MenuItemClicked { id: id.clone() });
    });

    if let Ok(mut callbacks) = MENU_BAR_CALLBACKS.lock() {
        callbacks.insert((hwnd as usize, win_id), callback);
    }
}

/// Adds a submenu to a menu.
unsafe fn add_submenu<T: Clone + Send + Sync + 'static>(
    hmenu: HMENU,
    submenu: &Submenu<T>,
    id_map: &mut IdMap,
    hwnd: HWND,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<(), anyhow::Error> {
    let child_hmenu = build_popup_menu(&submenu.items, id_map, hwnd, proxy, menu_bar_id)?;

    let mut flags = MF_POPUP;
    if !submenu.enabled {
        flags |= MF_GRAYED;
    }

    let label = encode_wide(&submenu.label);
    AppendMenuW(hmenu, flags, child_hmenu as usize, label.as_ptr());

    Ok(())
}

/// Recursively destroys a menu and all its submenus.
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
