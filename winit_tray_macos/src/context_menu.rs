use std::cell::RefCell;

use dpi::PhysicalPosition;
use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, MainThreadMarker};
use objc2_app_kit::{NSMenu, NSMenuItem, NSScreen};
use objc2_core_foundation::{CGPoint, CGSize};
use objc2_foundation::{NSObject, NSString};
use rwh_06::{HasWindowHandle, RawWindowHandle};
use winit_tray_core::MenuEntry;

// Thread-local storage for popup menu results
thread_local! {
    static POPUP_MENU_RESULT: RefCell<Option<usize>> = const { RefCell::new(None) };
}

// Popup menu target for context menus (uses tag-based identification)
struct PopupMenuTargetIvars;

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "WinitPopupMenuTarget"]
    #[ivars = PopupMenuTargetIvars]
    struct PopupMenuTarget;

    impl PopupMenuTarget {
        #[unsafe(method(menuItemClicked:))]
        fn menu_item_clicked(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            if tag > 0 {
                POPUP_MENU_RESULT.with(|result| {
                    *result.borrow_mut() = Some(tag as usize);
                });
            }
        }
    }
);

impl PopupMenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(PopupMenuTargetIvars);
        unsafe { msg_send![super(this), init] }
    }
}

/// Shows a context menu at the specified screen coordinates.
///
/// Returns the ID of the selected menu item, or None if the menu was dismissed.
pub fn show_context_menu_at_location<T: Clone>(
    mtm: MainThreadMarker,
    items: &[MenuEntry<T>],
    screen_x: f64,
    screen_y: f64,
) -> Option<T> {
    if items.is_empty() {
        return None;
    }

    // Build menu with tracking of item IDs
    let menu = NSMenu::new(mtm);
    let mut id_map: Vec<T> = Vec::new();
    let target = PopupMenuTarget::new(mtm);

    build_menu_for_popup(mtm, &menu, items, &mut id_map, &target);

    let location = CGPoint {
        x: screen_x,
        y: screen_y,
    };

    // Clear any previous result
    POPUP_MENU_RESULT.with(|result| {
        *result.borrow_mut() = None;
    });

    // This blocks until user makes a selection or dismisses
    // When view is None, location is in screen coordinates
    let _displayed = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

    // Check if an item was selected
    POPUP_MENU_RESULT.with(|result| {
        result.borrow_mut().take().and_then(|tag| {
            // Tag is 1-indexed, so subtract 1 to get the index
            if tag > 0 && tag <= id_map.len() {
                Some(id_map[tag - 1].clone())
            } else {
                None
            }
        })
    })
}

fn build_menu_for_popup<T: Clone>(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    items: &[MenuEntry<T>],
    id_map: &mut Vec<T>,
    target: &PopupMenuTarget,
) {
    for entry in items {
        match entry {
            MenuEntry::Separator => {
                let sep = NSMenuItem::separatorItem(mtm);
                menu.addItem(&sep);
            }
            MenuEntry::Item(item) => {
                let title = NSString::from_str(&item.label);
                let menu_item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        mtm.alloc(),
                        &title,
                        Some(sel!(menuItemClicked:)),
                        &NSString::from_str(""),
                    )
                };

                // Use tag to identify the item (1-indexed)
                let tag = id_map.len() + 1;
                menu_item.setTag(tag as isize);
                id_map.push(item.id.clone());

                // Set target and enabled state
                unsafe { menu_item.setTarget(Some(target)) };
                menu_item.setEnabled(item.enabled);

                // Set checked state if present
                if let Some(checked) = item.checked {
                    menu_item.setState(if checked { 1 } else { 0 });
                }

                menu.addItem(&menu_item);
            }
            MenuEntry::Submenu(submenu) => {
                let title = NSString::from_str(&submenu.label);
                let sub_item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        mtm.alloc(),
                        &title,
                        None,
                        &NSString::from_str(""),
                    )
                };

                let sub_menu = NSMenu::new(mtm);
                build_menu_for_popup(mtm, &sub_menu, &submenu.items, id_map, target);
                sub_item.setSubmenu(Some(&sub_menu));
                sub_item.setEnabled(submenu.enabled);
                menu.addItem(&sub_item);
            }
        }
    }
}

/// Show a context menu at the given position relative to a window.
///
/// The position is in client coordinates (relative to the window's content area).
/// Returns the selected menu item ID, or `None` if the menu was dismissed.
///
/// # Example
///
/// ```ignore
/// use winit_tray::{MenuEntry, MenuItem, show_context_menu_for_window};
///
/// let menu = vec![
///     MenuEntry::Item(MenuItem::new(1, "Option 1")),
///     MenuEntry::Separator,
///     MenuEntry::Item(MenuItem::new(2, "Option 2")),
/// ];
///
/// if let Some(id) = show_context_menu_for_window(&window, &menu, position) {
///     println!("Selected: {}", id);
/// }
/// ```
pub fn show_context_menu_for_window<T: Clone>(
    window: &impl HasWindowHandle,
    items: &[MenuEntry<T>],
    position: PhysicalPosition<i32>,
) -> Option<T> {
    let mtm = MainThreadMarker::new()?;
    let handle = window.window_handle().ok()?;

    match handle.as_raw() {
        RawWindowHandle::AppKit(appkit_handle) => {
            let ns_view = appkit_handle.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
            let ns_window: *mut objc2::runtime::AnyObject =
                unsafe { msg_send![ns_view, window] };
            if ns_view.is_null() || ns_window.is_null() {
                return None;
            }

            // Convert physical pixels to points
            let scale: f64 = unsafe { msg_send![ns_window, backingScaleFactor] };
            let x = position.x as f64 / scale;
            let y = position.y as f64 / scale;

            // Flip Y if view is not flipped (macOS uses bottom-left origin by default)
            let is_flipped: bool = unsafe { msg_send![ns_view, isFlipped] };
            let view_y = if is_flipped {
                y
            } else {
                let bounds: objc2_core_foundation::CGRect = unsafe { msg_send![ns_view, bounds] };
                bounds.size.height - y
            };

            // Convert view -> window -> screen coordinates
            let view_pt = CGPoint { x, y: view_y };
            let win_pt: CGPoint = unsafe {
                msg_send![ns_view, convertPoint: view_pt, toView: std::ptr::null::<objc2::runtime::AnyObject>()]
            };
            let rect = objc2_core_foundation::CGRect {
                origin: win_pt,
                size: CGSize { width: 0.0, height: 0.0 },
            };
            let screen_rect: objc2_core_foundation::CGRect =
                unsafe { msg_send![ns_window, convertRectToScreen: rect] };

            show_context_menu_at_location(mtm, items, screen_rect.origin.x, screen_rect.origin.y)
        }
        _ => None,
    }
}

/// Show a context menu at screen coordinates for any window that implements `HasWindowHandle`.
///
/// Similar to [`show_context_menu_for_window`], but the position is already in screen coordinates
/// (top-left origin, matching Windows convention).
///
/// Returns the selected menu item ID, or `None` if the menu was dismissed.
pub fn show_context_menu_for_window_at_screen_pos<T: Clone>(
    window: &impl HasWindowHandle,
    items: &[MenuEntry<T>],
    screen_position: PhysicalPosition<i32>,
) -> Option<T> {
    let mtm = MainThreadMarker::new()?;
    let handle = window.window_handle().ok()?;

    match handle.as_raw() {
        RawWindowHandle::AppKit(_) => {
            // Convert from top-left origin (Windows convention) to bottom-left origin (macOS)
            let screens = NSScreen::screens(mtm);
            let screen_height = if screens.count() > 0 {
                screens.objectAtIndex(0).frame().size.height
            } else {
                return None;
            };

            let location = CGPoint {
                x: screen_position.x as f64,
                y: screen_height - screen_position.y as f64,
            };

            show_context_menu_at_location(mtm, items, location.x, location.y)
        }
        _ => None,
    }
}
