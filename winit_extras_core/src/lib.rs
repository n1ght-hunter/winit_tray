//! Core types and traits for `winit_extras`.
//!
//! Defines the cross-platform event types, the `TrayIcon` and `ContextMenu`
//! traits, and the renderer factory traits (`TrayIconRenderer`, `MenuRenderer`)
//! that platform crates implement.

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

/// Events produced by tray icon clicks and context menu selections.
///
/// Delivered through the [`Manager`][`winit_extras::Manager`]'s event channel.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum Event<T = ()> {
    /// A pointer button was pressed or released on a tray icon.
    ///
    /// The `position` is in screen coordinates. Use `tray_icon_id` to
    /// distinguish events when multiple tray icons are active.
    PointerButton {
        tray_icon_id: tray_icon_id::TrayIconId,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
    },

    /// A menu item was clicked. Fires for both tray-triggered menus and
    /// programmatically-shown context menus.
    MenuItemClicked { id: T },
}

/// Shared callback used by platform backends to deliver [`Event`]s.
///
/// This is invoked from platform-specific threads (e.g. Win32 window proc,
/// D-Bus service thread), so the callback must be `Send + Sync`.
pub type EventCallback<T = ()> = std::sync::Arc<dyn Fn(Event<T>) + Send + Sync>;

/// Handle to a live tray icon.
///
/// Dropping the handle removes the icon from the system tray.
pub trait TrayIcon: std::fmt::Debug {
    /// Returns the unique ID for this tray icon.
    fn id(&self) -> tray_icon_id::TrayIconId;
}

/// Factory trait for creating tray icons.
///
/// Implementations plug a specific backend (native OS APIs, custom rendering,
/// etc.) into the [`Manager`][`winit_extras::Manager`]. The built-in
/// `NativeTrayIconRenderer` in each platform crate uses the OS-native tray API.
pub trait TrayIconRenderer<T: Clone + Send + Sync + 'static> {
    /// Create a tray icon with the given attributes.
    ///
    /// The `proxy` callback must be invoked by the backend for every click
    /// and interaction event produced by the tray icon.
    fn create_tray(
        &self,
        attributes: TrayIconAttributes,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn TrayIcon>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Configuration for creating a tray icon.
#[derive(Debug)]
pub struct TrayIconAttributes {
    /// Hover tooltip shown by the OS.
    pub tooltip: Option<String>,

    /// Window class name used internally on Windows.
    ///
    /// Ignored on other platforms. On Windows, this must be unique per process
    /// -- reusing the same class name as another window will cause the tray
    /// icon creation to fail.
    pub class_name: String,

    /// Icon displayed in the system tray.
    pub icon: Option<Icon>,

    /// Parent window handle.
    ///
    /// Currently only used on Windows, where the tray icon's hidden message
    /// window can be parented to an existing window.
    pub parent_window: Option<rwh_06::RawWindowHandle>,
}

impl Default for TrayIconAttributes {
    fn default() -> Self {
        TrayIconAttributes {
            tooltip: None,
            icon: None,
            class_name: "WinitExtrasTrayClass".to_string(),
            parent_window: None,
        }
    }
}

impl TrayIconAttributes {
    /// Set the tooltip text shown on hover.
    pub fn with_tooltip(mut self, title: impl Into<String>) -> Self {
        self.tooltip = Some(title.into());
        self
    }

    /// Set the icon displayed in the system tray.
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Override the Windows window class name.
    ///
    /// Must be unique per process on Windows. Ignored on other platforms.
    pub fn with_class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = class_name.into();
        self
    }

    /// Set the parent window handle (Windows only).
    pub fn with_parent_window(mut self, parent_window: rwh_06::RawWindowHandle) -> Self {
        self.parent_window = Some(parent_window);
        self
    }
}
