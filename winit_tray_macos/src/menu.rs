use std::cell::RefCell;
use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, MainThreadMarker};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSString};
use winit_tray_core::{MenuEntry, MenuItem, Submenu, TrayEvent, TrayProxy};

// Thread-local storage for menu item callbacks.
// Maps menu item pointer address to callback function.
thread_local! {
    static MENU_CALLBACKS: RefCell<HashMap<usize, Box<dyn Fn()>>> = RefCell::new(HashMap::new());
}

// Instance variables for MenuTarget (none needed, we use the address as key)
struct MenuTargetIvars;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "WinitMenuTarget"]
    #[ivars = MenuTargetIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(menuItemClicked:))]
        fn menu_item_clicked(&self, sender: &NSMenuItem) {
            let key = sender as *const NSMenuItem as usize;
            MENU_CALLBACKS.with(|callbacks| {
                if let Some(callback) = callbacks.borrow().get(&key) {
                    callback();
                }
            });
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(MenuTargetIvars);
        unsafe { msg_send![super(this), init] }
    }
}

// Retained menu targets to keep them alive
thread_local! {
    static MENU_TARGETS: RefCell<Vec<Retained<MenuTarget>>> = RefCell::new(Vec::new());
}

/// Creates an NSMenu from a vector of MenuEntry items.
pub(crate) fn create_menu<T: Clone + Send + Sync + 'static>(
    mtm: MainThreadMarker,
    entries: &[MenuEntry<T>],
    proxy: TrayProxy<T>,
    tray_id: winit_tray_core::tray_id::TrayId,
) -> Result<Option<Retained<NSMenu>>, anyhow::Error> {
    if entries.is_empty() {
        return Ok(None);
    }

    let menu = NSMenu::new(mtm);

    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                let separator = NSMenuItem::separatorItem(mtm);
                menu.addItem(&separator);
            }
            MenuEntry::Item(item) => {
                let menu_item = create_menu_item(mtm, item, proxy.clone(), tray_id)?;
                menu.addItem(&menu_item);
            }
            MenuEntry::Submenu(submenu) => {
                let submenu_item = create_submenu(mtm, submenu, proxy.clone(), tray_id)?;
                menu.addItem(&submenu_item);
            }
        }
    }

    Ok(Some(menu))
}

/// Creates a single NSMenuItem from a MenuItem.
fn create_menu_item<T: Clone + Send + Sync + 'static>(
    mtm: MainThreadMarker,
    item: &MenuItem<T>,
    proxy: TrayProxy<T>,
    tray_id: winit_tray_core::tray_id::TrayId,
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
    let target = MenuTarget::new(mtm);

    // Store callback
    let id = item.id.clone();
    let callback = Box::new(move || {
        proxy(tray_id, TrayEvent::MenuItemClicked { id: id.clone() });
    });

    let key = &*menu_item as *const NSMenuItem as usize;
    MENU_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().insert(key, callback);
    });

    // Set target and keep it alive
    unsafe { menu_item.setTarget(Some(&target)) };
    MENU_TARGETS.with(|targets| {
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
    proxy: TrayProxy<T>,
    tray_id: winit_tray_core::tray_id::TrayId,
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
    if let Some(submenu_menu) = create_menu(mtm, &submenu.items, proxy, tray_id)? {
        menu_item.setSubmenu(Some(&submenu_menu));
    }

    // Set enabled state
    menu_item.setEnabled(submenu.enabled);

    Ok(menu_item)
}
