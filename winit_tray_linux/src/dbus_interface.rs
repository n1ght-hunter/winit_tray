use crate::util::SniIcon;
use dpi::PhysicalPosition;
use std::sync::Arc;
use tracing::trace;
use winit_core::event::{ButtonSource, ElementState, MouseButton};
use winit_tray_core::{tray_id::TrayId, TrayEvent, TrayProxy};
use zbus::zvariant::ObjectPath;

/// StatusNotifierItem D-Bus interface implementation.
///
/// This struct holds the state for a tray icon and implements the
/// `org.kde.StatusNotifierItem` D-Bus interface.
pub struct StatusNotifierItemInterface<T> {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) icon_pixmap: Vec<SniIcon>,
    pub(crate) tray_id: TrayId,
    pub(crate) proxy: Arc<TrayProxy<T>>,
    pub(crate) menu: Option<ObjectPath<'static>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl<T: Clone + Send + Sync + 'static> StatusNotifierItemInterface<T> {
    /// Called when the user activates the tray icon (typically left-click).
    fn activate(&mut self, x: i32, y: i32) {
        trace!(x, y, "StatusNotifierItem::Activate called");

        let position = PhysicalPosition::new(x as f64, y as f64);
        let event = TrayEvent::PointerButton {
            state: ElementState::Released,
            position,
            button: ButtonSource::Mouse(MouseButton::Left),
        };

        (self.proxy)(self.tray_id, event);
    }

    /// Called when the user performs a secondary activation (typically right-click).
    fn secondary_activate(&mut self, x: i32, y: i32) {
        trace!(x, y, "StatusNotifierItem::SecondaryActivate called");

        let position = PhysicalPosition::new(x as f64, y as f64);
        let event = TrayEvent::PointerButton {
            state: ElementState::Released,
            position,
            button: ButtonSource::Mouse(MouseButton::Right),
        };

        (self.proxy)(self.tray_id, event);
    }

    /// Called when the user scrolls on the tray icon.
    fn scroll(&mut self, delta: i32, orientation: &str) {
        trace!(delta, orientation, "StatusNotifierItem::Scroll called");

        // Map scroll to middle button for now
        // TODO: Consider adding a dedicated scroll event type
        let position = PhysicalPosition::new(0.0, 0.0);
        let event = TrayEvent::PointerButton {
            state: ElementState::Released,
            position,
            button: ButtonSource::Mouse(MouseButton::Middle),
        };

        (self.proxy)(self.tray_id, event);
    }

    /// Unique identifier for this tray icon.
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.id
    }

    /// The title/tooltip for the tray icon.
    #[zbus(property)]
    fn title(&self) -> &str {
        &self.title
    }

    /// The category of the tray icon.
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    /// The status of the tray icon.
    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    /// Window ID (not used).
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    /// Theme icon name (empty - we use pixmaps).
    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }

    /// Icon pixmap data in ARGB32 format.
    #[zbus(property)]
    fn icon_pixmap(&self) -> &Vec<SniIcon> {
        &self.icon_pixmap
    }

    /// Overlay icon name (not used).
    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    /// Overlay icon pixmap (not used).
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<SniIcon> {
        vec![]
    }

    /// Attention icon name (not used).
    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    /// Attention icon pixmap (not used).
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<SniIcon> {
        vec![]
    }

    /// Attention movie name (not used).
    #[zbus(property)]
    fn attention_movie_name(&self) -> &str {
        ""
    }

    /// Tooltip information.
    /// Format: (icon_name, icon_pixmap, title, description)
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<SniIcon>, String, String) {
        (
            String::new(),
            vec![],
            self.title.clone(),
            String::new(),
        )
    }

    /// Icon theme path (not used).
    #[zbus(property)]
    fn icon_theme_path(&self) -> &str {
        ""
    }

    /// Menu object path (if menu feature is enabled).
    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> {
        self.menu.clone().unwrap_or_else(|| {
            ObjectPath::try_from("/").expect("Invalid root path")
        })
    }

    /// Whether the item is a menu itself.
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
}
