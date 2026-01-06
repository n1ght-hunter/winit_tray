//! Menu bar implementation for macOS.
//!
//! On macOS, the menu bar is a global application menu bar managed by NSApplication.

use std::cell::RefCell;
use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSString};
use winit_tray_core::menu_bar::{
    MenuBar as CoreMenuBar, MenuBarAttributes, MenuBarEvent, MenuBarId, MenuBarProxy,
    TopLevelMenu,
};
use winit_tray_core::{MenuEntry, MenuItem, Submenu};

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

// Thread-local storage for menu bar item callbacks.
thread_local! {
    static MENU_BAR_CALLBACKS: RefCell<HashMap<usize, Box<dyn Fn()>>> = RefCell::new(HashMap::new());
}

// Instance variables for MenuBarTarget
struct MenuBarTargetIvars;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "WinitMenuBarTarget"]
    #[ivars = MenuBarTargetIvars]
    struct MenuBarTarget;

    impl MenuBarTarget {
        #[unsafe(method(menuItemClicked:))]
        fn menu_item_clicked(&self, sender: &NSMenuItem) {
            let key = sender as *const NSMenuItem as usize;
            MENU_BAR_CALLBACKS.with(|callbacks| {
                if let Some(callback) = callbacks.borrow().get(&key) {
                    callback();
                }
            });
        }
    }
);

impl MenuBarTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(MenuBarTargetIvars);
        unsafe { msg_send![super(this), init] }
    }
}

// Retained menu bar targets to keep them alive
thread_local! {
    static MENU_BAR_TARGETS: RefCell<Vec<Retained<MenuBarTarget>>> = RefCell::new(Vec::new());
}

/// macOS menu bar implementation.
pub struct MenuBar {
    internal_id: usize,
    #[allow(dead_code)] // Kept to hold ownership of the menu
    main_menu: Retained<NSMenu>,
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
    pub fn new<T: Clone + Send + Sync + 'static>(
        proxy: MenuBarProxy<T>,
        attr: MenuBarAttributes<T>,
    ) -> Result<Self, anyhow::Error> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow::anyhow!("MenuBar must be created on the main thread"))?;

        let internal_id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let menu_bar_id = MenuBarId::from_raw(internal_id);

        // Create the main menu
        let main_menu = NSMenu::new(mtm);

        // Add top-level menus
        for top_level in &attr.menus {
            let menu_item = create_top_level_menu(mtm, top_level, proxy.clone(), menu_bar_id)?;
            main_menu.addItem(&menu_item);
        }

        // Set as the application's main menu
        let app = NSApplication::sharedApplication(mtm);
        app.setMainMenu(Some(&main_menu));

        Ok(MenuBar {
            internal_id,
            main_menu,
        })
    }
}

impl CoreMenuBar for MenuBar {
    fn id(&self) -> MenuBarId {
        MenuBarId::from_raw(self.internal_id)
    }

    fn remove(&self) {
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            // Set an empty menu to clear the menu bar
            let empty_menu = NSMenu::new(mtm);
            app.setMainMenu(Some(&empty_menu));
        }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        // Clean up menu bar callbacks associated with this menu bar
        // Note: We don't remove the main menu on drop since it would leave the app without a menu
    }
}

/// Creates an NSMenuItem for a top-level menu.
fn create_top_level_menu<T: Clone + Send + Sync + 'static>(
    mtm: MainThreadMarker,
    top_level: &TopLevelMenu<T>,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<Retained<NSMenuItem>, anyhow::Error> {
    let title = NSString::from_str(&top_level.label);
    let menu_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &title,
            None,
            &NSString::from_str(""),
        )
    };

    // Create the submenu for this top-level menu
    let submenu = create_menu(mtm, &top_level.items, proxy, menu_bar_id)?;
    submenu.setTitle(&title);
    menu_item.setSubmenu(Some(&submenu));

    Ok(menu_item)
}

/// Creates an NSMenu from a vector of MenuEntry items.
fn create_menu<T: Clone + Send + Sync + 'static>(
    mtm: MainThreadMarker,
    entries: &[MenuEntry<T>],
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<Retained<NSMenu>, anyhow::Error> {
    let menu = NSMenu::new(mtm);

    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                let separator = NSMenuItem::separatorItem(mtm);
                menu.addItem(&separator);
            }
            MenuEntry::Item(item) => {
                let menu_item = create_menu_item(mtm, item, proxy.clone(), menu_bar_id)?;
                menu.addItem(&menu_item);
            }
            MenuEntry::Submenu(submenu) => {
                let submenu_item = create_submenu(mtm, submenu, proxy.clone(), menu_bar_id)?;
                menu.addItem(&submenu_item);
            }
        }
    }

    Ok(menu)
}

/// Creates a single NSMenuItem from a MenuItem.
fn create_menu_item<T: Clone + Send + Sync + 'static>(
    mtm: MainThreadMarker,
    item: &MenuItem<T>,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<Retained<NSMenuItem>, anyhow::Error> {
    let title = NSString::from_str(&item.label);
    let menu_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &title,
            Some(sel!(menuItemClicked:)),
            &NSString::from_str(""),
        )
    };

    // Create target for this menu item
    let target = MenuBarTarget::new(mtm);

    // Store callback
    let id = item.id.clone();
    let callback = Box::new(move || {
        proxy(menu_bar_id, MenuBarEvent::MenuItemClicked { id: id.clone() });
    });

    let key = &*menu_item as *const NSMenuItem as usize;
    MENU_BAR_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().insert(key, callback);
    });

    // Set target and keep it alive
    unsafe { menu_item.setTarget(Some(&target)) };
    MENU_BAR_TARGETS.with(|targets| {
        targets.borrow_mut().push(target);
    });

    // Set enabled state
    menu_item.setEnabled(item.enabled);

    Ok(menu_item)
}

/// Creates a submenu NSMenuItem from a Submenu.
fn create_submenu<T: Clone + Send + Sync + 'static>(
    mtm: MainThreadMarker,
    submenu: &Submenu<T>,
    proxy: MenuBarProxy<T>,
    menu_bar_id: MenuBarId,
) -> Result<Retained<NSMenuItem>, anyhow::Error> {
    let title = NSString::from_str(&submenu.label);
    let menu_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &title,
            None,
            &NSString::from_str(""),
        )
    };

    // Create the submenu
    let submenu_menu = create_menu(mtm, &submenu.items, proxy, menu_bar_id)?;
    submenu_menu.setTitle(&title);
    menu_item.setSubmenu(Some(&submenu_menu));

    // Set enabled state
    menu_item.setEnabled(submenu.enabled);

    Ok(menu_item)
}
