use winit::{
    dpi::PhysicalPosition,
    event::{ButtonSource, ElementState},
    icon::Icon,
};

#[cfg(feature = "menu")]
pub mod menu;
#[cfg(feature = "menu")]
pub use menu::*;

#[cfg(feature = "context_menu")]
pub mod context_menu;

#[cfg(feature = "menu_bar")]
pub mod menu_bar;

pub mod tray_icon_id;

/// Events from the tray system -- tray icon clicks and menu item selections.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Event<T = ()> {
    /// A pointer button was pressed or released on the tray icon.
    PointerButton {
        tray_icon_id: tray_icon_id::TrayIconId,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
    },

    /// A menu item was clicked (from any context menu).
    MenuItemClicked { id: T },
}

/// Callback for delivering tray events from platform threads.
pub type EventCallback<T = ()> = std::sync::Arc<dyn Fn(Event<T>) + Send + Sync>;

/// Handle to a tray icon.
pub trait TrayIcon: std::fmt::Debug {
    fn id(&self) -> tray_icon_id::TrayIconId;
}

/// Factory for creating tray icons.
pub trait TrayIconRenderer<T: Clone + Send + Sync + 'static> {
    fn create_tray(
        &self,
        attributes: TrayIconAttributes,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn TrayIcon>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Configuration for creating a tray icon.
#[derive(Debug)]
pub struct TrayIconAttributes {
    pub tooltip: Option<String>,
    pub class_name: String,
    pub icon: Option<Icon>,
    pub parent_window: Option<rwh_06::RawWindowHandle>,
}

impl Default for TrayIconAttributes {
    fn default() -> Self {
        TrayIconAttributes {
            tooltip: None,
            icon: None,
            class_name: "Window Tray Class".to_string(),
            parent_window: None,
        }
    }
}

impl TrayIconAttributes {
    pub fn with_tooltip(mut self, title: impl Into<String>) -> Self {
        self.tooltip = Some(title.into());
        self
    }

    pub fn with_icon(mut self, icon: Option<Icon>) -> Self {
        self.icon = icon;
        self
    }

    /// WARNING: On Windows if this is the same as another window class name, it will cause issues.
    pub fn with_class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = class_name.into();
        self
    }

    pub fn with_parent_window(mut self, parent_window: rwh_06::RawWindowHandle) -> Self {
        self.parent_window = Some(parent_window);
        self
    }
}
