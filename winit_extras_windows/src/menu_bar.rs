//! Menu bar implementation for Windows.
//!
//! On Windows, the menu bar is attached to a window using SetMenu().

use std::collections::HashMap;
use std::ptr;

use rwh_06::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            AppendMenuW, CreateMenu, CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetSubMenu,
            HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, SetMenu, WM_COMMAND,
            WM_NCDESTROY,
        },
    },
};
use winit_extras_core::menu_bar::{
    MenuBar as CoreMenuBar, MenuBarAttributes, MenuBarEvent, MenuBarId, MenuBarProxy, TopLevelMenu,
};
use winit_extras_core::{MenuEntry, MenuItem, Submenu};

use crate::util::encode_wide;

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

const MENUBAR_SUBCLASS_ID: usize = 0x4D454E55; // "MENU" in hex

struct MenuBarState<T> {
    id_map: HashMap<u32, T>,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
}

impl<T: Clone + Send + Sync + 'static> MenuBarState<T> {
    fn handle_command(&self, command_id: u32) -> bool {
        if let Some(id) = self.id_map.get(&command_id) {
            (self.proxy)(
                self.menu_bar_id,
                MenuBarEvent::MenuItemClicked { id: id.clone() },
            );
            return true;
        }
        false
    }
}

type CleanupFn = unsafe fn(HWND, *mut ());

/// Windows menu bar implementation.
pub struct MenuBar {
    internal_id: usize,
    hwnd: HWND,
    hmenu: HMENU,
    state_ptr: *mut (),
    cleanup: CleanupFn,
}

unsafe impl Send for MenuBar {}
unsafe impl Sync for MenuBar {}

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
                ));
            }
            None => return Err(anyhow::anyhow!("parent_window is required on Windows")),
        };

        let internal_id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let menu_bar_id = MenuBarId::from_raw(internal_id);

        let mut state = Box::new(MenuBarState {
            id_map: HashMap::new(),
            proxy,
            menu_bar_id,
        });

        let mut next_id: u32 = 1;

        // Create the main menu bar
        let hmenu = unsafe { CreateMenu() };
        if hmenu.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        // Add top-level menus
        for top_level in &attr.menus {
            unsafe {
                add_top_level_menu(hmenu, top_level, &mut next_id, &mut state)?;
            }
        }

        // Attach the menu bar to the window
        if unsafe { SetMenu(hwnd, hmenu) } == 0 {
            unsafe { destroy_menu_tree(hmenu) };
            return Err(std::io::Error::last_os_error().into());
        }

        // Install window subclass to handle WM_COMMAND
        let state_ptr = Box::into_raw(state);
        let result = unsafe {
            SetWindowSubclass(
                hwnd,
                Some(menubar_subclass_proc::<T>),
                MENUBAR_SUBCLASS_ID,
                state_ptr as usize,
            )
        };

        if result == 0 {
            unsafe {
                SetMenu(hwnd, ptr::null_mut());
                destroy_menu_tree(hmenu);
                drop(Box::from_raw(state_ptr));
            }
            return Err(anyhow::anyhow!("Failed to install window subclass"));
        }

        Ok(MenuBar {
            internal_id,
            hwnd,
            hmenu,
            state_ptr: state_ptr as *mut (),
            cleanup: cleanup_subclass::<T>,
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

unsafe fn cleanup_subclass<T: Clone + Send + Sync + 'static>(hwnd: HWND, state_ptr: *mut ()) {
    unsafe { RemoveWindowSubclass(hwnd, Some(menubar_subclass_proc::<T>), MENUBAR_SUBCLASS_ID) };

    if !state_ptr.is_null() {
        let state = state_ptr as *mut MenuBarState<T>;
        drop(unsafe { Box::from_raw(state) });
    }
}

impl CoreMenuBar for MenuBar {
    fn id(&self) -> MenuBarId {
        MenuBarId::from_raw(self.internal_id)
    }

    fn remove(&self) {
        unsafe {
            SetMenu(self.hwnd, ptr::null_mut());
        }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        unsafe {
            SetMenu(self.hwnd, ptr::null_mut());
            (self.cleanup)(self.hwnd, self.state_ptr);
            destroy_menu_tree(self.hmenu);
        }
    }
}

unsafe extern "system" fn menubar_subclass_proc<T: Clone + Send + Sync + 'static>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    dw_ref_data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY && dw_ref_data != 0 {
        let state = dw_ref_data as *mut MenuBarState<T>;
        drop(unsafe { Box::from_raw(state) });
        unsafe {
            RemoveWindowSubclass(hwnd, Some(menubar_subclass_proc::<T>), MENUBAR_SUBCLASS_ID)
        };
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    }

    if dw_ref_data == 0 {
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    }

    let state = unsafe { &*(dw_ref_data as *const MenuBarState<T>) };

    if msg == WM_COMMAND {
        let command_id = (wparam & 0xFFFF) as u32;
        let notification_code = ((wparam >> 16) & 0xFFFF) as u16;

        // notification_code == 0 means menu item, notification_code == 1 means accelerator
        if (notification_code == 0 || notification_code == 1) && state.handle_command(command_id) {
            return 0;
        }
    }

    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

unsafe fn add_top_level_menu<T: Clone + Send + Sync + 'static>(
    hmenu_bar: HMENU,
    top_level: &TopLevelMenu<T>,
    next_id: &mut u32,
    state: &mut MenuBarState<T>,
) -> Result<(), anyhow::Error> {
    let hmenu_popup = unsafe { build_popup_menu(&top_level.items, next_id, state)? };

    let label = encode_wide(&top_level.label);
    unsafe { AppendMenuW(hmenu_bar, MF_POPUP, hmenu_popup as usize, label.as_ptr()) };

    Ok(())
}

unsafe fn build_popup_menu<T: Clone + Send + Sync + 'static>(
    items: &[MenuEntry<T>],
    next_id: &mut u32,
    state: &mut MenuBarState<T>,
) -> Result<HMENU, anyhow::Error> {
    let hmenu = unsafe { CreatePopupMenu() };
    if hmenu.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }

    for item in items {
        match item {
            MenuEntry::Item(item) => {
                unsafe { add_menu_item(hmenu, item, next_id, state) };
            }
            MenuEntry::Submenu(submenu) => {
                unsafe { add_submenu(hmenu, submenu, next_id, state)? };
            }
            MenuEntry::Separator => {
                unsafe { AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null()) };
            }
        }
    }

    Ok(hmenu)
}

unsafe fn add_menu_item<T: Clone + Send + Sync + 'static>(
    hmenu: HMENU,
    item: &MenuItem<T>,
    next_id: &mut u32,
    state: &mut MenuBarState<T>,
) {
    let mut flags = MF_STRING;
    if !item.enabled {
        flags |= MF_GRAYED;
    }
    if item.checked == Some(true) {
        flags |= MF_CHECKED;
    }

    let win_id = *next_id;
    *next_id += 1;

    let label = encode_wide(&item.label);
    unsafe { AppendMenuW(hmenu, flags, win_id as usize, label.as_ptr()) };

    state.id_map.insert(win_id, item.id.clone());
}

unsafe fn add_submenu<T: Clone + Send + Sync + 'static>(
    hmenu: HMENU,
    submenu: &Submenu<T>,
    next_id: &mut u32,
    state: &mut MenuBarState<T>,
) -> Result<(), anyhow::Error> {
    let child_hmenu = unsafe { build_popup_menu(&submenu.items, next_id, state)? };

    let mut flags = MF_POPUP;
    if !submenu.enabled {
        flags |= MF_GRAYED;
    }

    let label = encode_wide(&submenu.label);
    unsafe { AppendMenuW(hmenu, flags, child_hmenu as usize, label.as_ptr()) };

    Ok(())
}

/// Recursively destroys a menu and all its submenus.
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
