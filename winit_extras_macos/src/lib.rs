#![cfg(target_os = "macos")]
mod util;

#[cfg(feature = "menu")]
pub mod menu;

#[cfg(feature = "context_menu")]
pub mod context_menu;

#[cfg(feature = "menu_bar")]
pub mod menu_bar;

use dpi::PhysicalPosition;
use objc2::rc::Retained;
use objc2::{define_class, msg_send, AllocAnyThread, DeclaredClass, MainThreadMarker};
#[cfg(feature = "menu")]
use objc2_app_kit::NSMenu;
use objc2_app_kit::{
    NSEvent, NSStatusBar, NSStatusItem, NSTrackingArea, NSTrackingAreaOptions,
    NSVariableStatusItemLength, NSView,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
#[cfg(feature = "menu")]
use std::cell::Cell;
use tracing::trace;
use winit_core::event::{ElementState, MouseButton};
use winit_extras_core::{Event, EventCallback, TrayIcon as CoreTrayIcon, TrayIconAttributes};

use winit_extras_core::{Event, EventCallback, TrayIconAttributes, TrayIconRenderer};

use crate::util::icon_to_nsimage;

/// Uses native macOS `NSStatusBar` / `NSStatusItem` APIs.
pub struct NativeTrayIconRenderer;

impl<T: Clone + Send + Sync + 'static> TrayIconRenderer<T> for NativeTrayIconRenderer {
    fn create_tray(
        &self,
        attributes: TrayIconAttributes,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn winit_extras_core::TrayIcon>, Box<dyn std::error::Error + Send + Sync>>
    {
        let tray = Tray::new(proxy, attributes)?;
        Ok(Box::new(tray))
    }
}

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

pub struct Tray<T = ()> {
    status_item: Retained<NSStatusItem>,
    tray_target: Retained<TrayTarget>,
    internal_id: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T> std::fmt::Debug for Tray<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("internal_id", &self.internal_id)
            .finish_non_exhaustive()
    }
}

// Instance variables for TrayTarget
struct TrayTargetIvars {
    tray_icon_id: usize,
    status_item: Retained<NSStatusItem>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[name = "WinitTrayTarget"]
    #[ivars = TrayTargetIvars]
    struct TrayTarget;

    /// Mouse events on NSResponder
    impl TrayTarget {
        #[unsafe(method(mouseDown:))]
        fn on_mouse_down(&self, event: &NSEvent) {
            self.send_mouse_event(event, MouseButton::Left, ElementState::Pressed);
            self.on_tray_click(MouseButton::Left);
        }

        #[unsafe(method(mouseUp:))]
        fn on_mouse_up(&self, event: &NSEvent) {
            let mtm = MainThreadMarker::from(self);
            let button = self.ivars().status_item.button(mtm).unwrap();
            button.highlight(false);
            self.send_mouse_event(event, MouseButton::Left, ElementState::Released);
        }

        #[unsafe(method(rightMouseDown:))]
        fn on_right_mouse_down(&self, event: &NSEvent) {
            self.send_mouse_event(event, MouseButton::Right, ElementState::Pressed);
            self.on_tray_click(MouseButton::Right);
        }

        #[unsafe(method(rightMouseUp:))]
        fn on_right_mouse_up(&self, event: &NSEvent) {
            self.send_mouse_event(event, MouseButton::Right, ElementState::Released);
        }

        #[unsafe(method(otherMouseDown:))]
        fn on_other_mouse_down(&self, event: &NSEvent) {
            let button_number = event.buttonNumber();
            if button_number == 2 {
                self.send_mouse_event(event, MouseButton::Middle, ElementState::Pressed);
            }
        }

        #[unsafe(method(otherMouseUp:))]
        fn on_other_mouse_up(&self, event: &NSEvent) {
            let button_number = event.buttonNumber();
            if button_number == 2 {
                self.send_mouse_event(event, MouseButton::Middle, ElementState::Released);
            }
        }
    }

    /// Tracking mouse enter/exit/move events
    impl TrayTarget {
        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            let areas = self.trackingAreas();
            for area in areas {
                self.removeTrackingArea(&area);
            }

            let _: () = unsafe { msg_send![super(self), updateTrackingAreas] };

            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect;
            let rect = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: 0.0,
                    height: 0.0,
                },
            };
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    rect,
                    options,
                    Some(self),
                    None,
                )
            };
            self.addTrackingArea(&area);
        }
    }
);

impl TrayTarget {
    fn update_dimensions(&self) {
        let mtm = MainThreadMarker::from(self);
        let button = self.ivars().status_item.button(mtm).unwrap();
        self.setFrame(button.frame());
    }

    fn send_mouse_event(&self, _event: &NSEvent, button: MouseButton, state: ElementState) {
        let tray_icon_id =
            winit_extras_core::tray_icon_id::TrayIconId::from_raw(self.ivars().tray_icon_id);

        // Get cursor position
        let mouse_location = NSEvent::mouseLocation();
        let position = PhysicalPosition::new(mouse_location.x, mouse_location.y);

        trace!(?button, ?state, ?position, "Tray mouse event");

        TRAY_EVENT_HANDLER.with(|handler| {
            if let Some(handler) = handler.borrow().as_ref() {
                handler(Event::PointerButton {
                    tray_icon_id,
                    state,
                    position,
                    button: winit_core::event::ButtonSource::Mouse(button),
                });
            }
        });
    }

    fn on_tray_click(&self, _button: MouseButton) {
        let mtm = MainThreadMarker::from(self);
        let ns_button = self.ivars().status_item.button(mtm).unwrap();
        ns_button.highlight(true);
    }
}

// Thread-local storage for the event handler callback
// This is necessary because we can't pass closures through Objective-C
thread_local! {
    static TRAY_EVENT_HANDLER: std::cell::RefCell<Option<Box<dyn Fn(Event<()>) + Send + Sync>>> = std::cell::RefCell::new(None);
}

impl<T: Clone + Send + Sync + 'static> Tray<T> {
    pub fn new(proxy: EventCallback<T>, attr: TrayIconAttributes) -> Result<Self, anyhow::Error> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow::anyhow!("Tray must be created on the main thread"))?;

        let internal_id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Set up the event handler
        let proxy_clone = proxy.clone();
        TRAY_EVENT_HANDLER.with(|handler| {
            *handler.borrow_mut() = Some(Box::new(move |event| {
                let typed_event = match event {
                    Event::PointerButton {
                        tray_icon_id,
                        state,
                        position,
                        button,
                    } => Event::PointerButton {
                        tray_icon_id,
                        state,
                        position,
                        button,
                    },
                    _ => return,
                };
                (proxy_clone)(typed_event);
            }));
        });

        // Create status item
        let status_item =
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);

        // Get the button
        let button = status_item
            .button(mtm)
            .ok_or_else(|| anyhow::anyhow!("Failed to get status item button"))?;

        // Set the icon if provided
        if let Some(icon) = attr.icon.as_ref() {
            if let Some(nsimage) = icon_to_nsimage(icon) {
                button.setImage(Some(&nsimage));
            }
        }

        // Set the tooltip if provided
        if let Some(tooltip) = &attr.tooltip {
            let ns_tooltip = NSString::from_str(tooltip);
            button.setToolTip(Some(&ns_tooltip));
        }

        // Create the TrayTarget view and add it to the button
        let frame = button.frame();

        let target = mtm.alloc().set_ivars(TrayTargetIvars {
            tray_icon_id: internal_id,
            status_item: status_item.clone(),
        });

        let tray_target: Retained<TrayTarget> =
            unsafe { msg_send![super(target), initWithFrame: frame] };
        tray_target.setWantsLayer(true);

        button.addSubview(&tray_target);

        Ok(Tray {
            status_item,
            tray_target,
            internal_id,
            _marker: std::marker::PhantomData,
        })
    }

    pub fn set_tooltip(&self, tooltip: Option<&str>) -> Result<(), anyhow::Error> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow::anyhow!("set_tooltip must be called on the main thread"))?;

        if let Some(button) = self.status_item.button(mtm) {
            if let Some(tooltip_str) = tooltip {
                let ns_tooltip = NSString::from_str(tooltip_str);
                button.setToolTip(Some(&ns_tooltip));
            } else {
                button.setToolTip(None);
            }
            self.tray_target.update_dimensions();
        }

        Ok(())
    }
}

impl<T: Send + Sync> CoreTrayIcon for Tray<T> {
    fn id(&self) -> winit_extras_core::tray_icon_id::TrayIconId {
        winit_extras_core::tray_icon_id::TrayIconId::from_raw(self.internal_id)
    }
}

impl<T> Drop for Tray<T> {
    fn drop(&mut self) {
        // NSStatusItem must be removed on the main thread
        if let Some(_mtm) = MainThreadMarker::new() {
            NSStatusBar::systemStatusBar().removeStatusItem(&self.status_item);
            self.tray_target.removeFromSuperview();
        } else {
            tracing::warn!("Tray dropped from non-main thread, status item will leak");
        }
    }
}
