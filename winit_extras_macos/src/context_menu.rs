use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use dpi::PhysicalPosition;
use objc2::rc::Retained;
use objc2::{define_class, msg_send, sel, MainThreadMarker};
use objc2_app_kit::{NSMenu, NSMenuItem, NSScreen};
use objc2_core_foundation::{CGPoint, CGSize};
use objc2_foundation::{NSObject, NSString};
use rwh_06::{HasWindowHandle, RawWindowHandle};
use winit_core::event_loop::ActiveEventLoop;
use winit_extras_core::context_menu::{ContextMenu as ContextMenuTrait, MenuRenderer};
use winit_extras_core::{Event, EventCallback, MenuEntry};

// Thread-local storage for popup menu results
thread_local! {
    static POPUP_MENU_RESULT: RefCell<Option<usize>> = const { RefCell::new(None) };
}

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

fn show_context_menu_at_location<T: Clone>(
    mtm: MainThreadMarker,
    items: &[MenuEntry<T>],
    screen_x: f64,
    screen_y: f64,
) -> Option<T> {
    if items.is_empty() {
        return None;
    }

    let menu = NSMenu::new(mtm);
    let mut id_map: Vec<T> = Vec::new();
    let target = PopupMenuTarget::new(mtm);

    build_menu_for_popup(mtm, &menu, items, &mut id_map, &target);

    let location = CGPoint {
        x: screen_x,
        y: screen_y,
    };

    POPUP_MENU_RESULT.with(|result| {
        *result.borrow_mut() = None;
    });

    let _displayed = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

    POPUP_MENU_RESULT.with(|result| {
        result.borrow_mut().take().and_then(|tag| {
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

                let tag = id_map.len() + 1;
                menu_item.setTag(tag as isize);
                id_map.push(item.id.clone());

                unsafe { menu_item.setTarget(Some(target)) };
                menu_item.setEnabled(item.enabled);

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
            let ns_window: *mut objc2::runtime::AnyObject = unsafe { msg_send![ns_view, window] };
            if ns_view.is_null() || ns_window.is_null() {
                return None;
            }

            let scale: f64 = unsafe { msg_send![ns_window, backingScaleFactor] };
            let x = position.x as f64 / scale;
            let y = position.y as f64 / scale;

            let is_flipped: bool = unsafe { msg_send![ns_view, isFlipped] };
            let view_y = if is_flipped {
                y
            } else {
                let bounds: objc2_core_foundation::CGRect = unsafe { msg_send![ns_view, bounds] };
                bounds.size.height - y
            };

            let view_pt = CGPoint { x, y: view_y };
            let win_pt: CGPoint = unsafe {
                msg_send![ns_view, convertPoint: view_pt, toView: std::ptr::null::<objc2::runtime::AnyObject>()]
            };
            let rect = objc2_core_foundation::CGRect {
                origin: win_pt,
                size: CGSize {
                    width: 0.0,
                    height: 0.0,
                },
            };
            let screen_rect: objc2_core_foundation::CGRect =
                unsafe { msg_send![ns_window, convertRectToScreen: rect] };

            show_context_menu_at_location(mtm, items, screen_rect.origin.x, screen_rect.origin.y)
        }
        _ => None,
    }
}

/// Menu alignment options (for API compatibility with Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAlignment {
    Auto,
}

pub struct ContextMenu<T> {
    items: Vec<MenuEntry<T>>,
    proxy: EventCallback<T>,
    ns_view: *mut objc2::runtime::AnyObject,
}

impl<T> std::fmt::Debug for ContextMenu<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextMenu").finish_non_exhaustive()
    }
}

unsafe impl<T: Send> Send for ContextMenu<T> {}
unsafe impl<T: Sync> Sync for ContextMenu<T> {}

impl<T: Clone + Send + Sync + 'static> ContextMenu<T> {
    pub fn new(
        window: &(impl HasWindowHandle + ?Sized),
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Self, anyhow::Error> {
        let handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Failed to get window handle: {}", e))?;

        let ns_view = match handle.as_raw() {
            RawWindowHandle::AppKit(appkit_handle) => {
                appkit_handle.ns_view.as_ptr() as *mut objc2::runtime::AnyObject
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid window handle type, expected AppKit"
                ))
            }
        };

        Ok(Self {
            items,
            proxy,
            ns_view,
        })
    }

    fn show_at_screen_pos_internal(&self, screen_x: f64, screen_y: f64) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };

        let result = show_context_menu_at_location(mtm, &self.items, screen_x, screen_y);
        if let Some(id) = result {
            (self.proxy)(Event::MenuItemClicked { id });
        }
    }
}

impl<T: Clone + Send + Sync + 'static> ContextMenuTrait for ContextMenu<T> {
    fn show(&self, position: PhysicalPosition<i32>) {
        if self.ns_view.is_null() {
            return;
        }

        let ns_window: *mut objc2::runtime::AnyObject = unsafe { msg_send![self.ns_view, window] };
        if ns_window.is_null() {
            return;
        }

        let scale: f64 = unsafe { msg_send![ns_window, backingScaleFactor] };
        let x = position.x as f64 / scale;
        let y = position.y as f64 / scale;

        let is_flipped: bool = unsafe { msg_send![self.ns_view, isFlipped] };
        let view_y = if is_flipped {
            y
        } else {
            let bounds: objc2_core_foundation::CGRect = unsafe { msg_send![self.ns_view, bounds] };
            bounds.size.height - y
        };

        let view_pt = CGPoint { x, y: view_y };
        let win_pt: CGPoint = unsafe {
            msg_send![self.ns_view, convertPoint: view_pt, toView: std::ptr::null::<objc2::runtime::AnyObject>()]
        };
        let rect = objc2_core_foundation::CGRect {
            origin: win_pt,
            size: CGSize {
                width: 0.0,
                height: 0.0,
            },
        };
        let screen_rect: objc2_core_foundation::CGRect =
            unsafe { msg_send![ns_window, convertRectToScreen: rect] };

        self.show_at_screen_pos_internal(screen_rect.origin.x, screen_rect.origin.y);
    }

    fn show_at_screen_pos(&self, position: PhysicalPosition<i32>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };

        let screens = NSScreen::screens(mtm);
        if screens.count() == 0 {
            return;
        }
        let screen_height = screens.objectAtIndex(0).frame().size.height;

        self.show_at_screen_pos_internal(position.x as f64, screen_height - position.y as f64);
    }

    fn close(&self) {}
}

/// Uses native macOS `NSMenu` popup menus.
pub struct NativeMenuRenderer;

impl<T: Clone + Send + Sync + 'static> MenuRenderer<T> for NativeMenuRenderer {
    fn create_menu(
        &self,
        _event_loop: &dyn ActiveEventLoop,
        window: &dyn HasWindowHandle,
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn ContextMenuTrait>, Box<dyn std::error::Error + Send + Sync>> {
        let menu = ContextMenu::new(window, items, proxy)?;
        Ok(Box::new(menu))
    }
}
