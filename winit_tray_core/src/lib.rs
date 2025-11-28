use winit::{
    dpi::PhysicalPosition,
    event::{ButtonSource, ElementState},
    icon::Icon,
};
#[cfg(feature = "menu")]
pub use winit::event::MouseButton;

#[cfg(feature = "menu")]
pub mod menu;
#[cfg(feature = "menu")]
pub use menu::*;

pub mod tray_id;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum TrayEvent<T = ()> {
    PointerButton {
        state: ElementState,

        /// The position of the pointer when the button was pressed.
        ///
        /// ## Platform-specific
        ///
        /// - **Orbital: Always emits `(0., 0.)`.
        /// - **Web:** Doesn't take into account CSS [`border`], [`padding`], or [`transform`].
        ///
        /// [`border`]: https://developer.mozilla.org/en-US/docs/Web/CSS/border
        /// [`padding`]: https://developer.mozilla.org/en-US/docs/Web/CSS/padding
        /// [`transform`]: https://developer.mozilla.org/en-US/docs/Web/CSS/transform
        position: PhysicalPosition<f64>,

        button: ButtonSource,
    },

    /// A menu item was clicked.
    #[cfg(feature = "menu")]
    MenuItemClicked {
        /// The ID of the clicked menu item.
        id: T,
    },

    /// Phantom variant to keep the type parameter used when menu feature is disabled.
    #[doc(hidden)]
    #[cfg(not(feature = "menu"))]
    __Phantom(std::marker::PhantomData<T>),
}

pub type TrayProxy<T = ()> = std::sync::Arc<dyn Fn(tray_id::TrayId, TrayEvent<T>) + Send + Sync>;

pub trait Tray: std::fmt::Debug {
    fn id(&self) -> tray_id::TrayId;
}

#[derive(Debug)]
pub struct TrayAttributes<T = ()> {
    pub tooltip: Option<String>,
    pub class_name: String,
    pub icon: Option<Icon>,
    pub parent_window: Option<rwh_06::RawWindowHandle>,
    #[cfg(feature = "menu")]
    pub menu: Option<Vec<MenuEntry<T>>>,
    /// Which mouse button opens the menu. Defaults to `MouseButton::Right`.
    #[cfg(feature = "menu")]
    pub menu_on_button: MouseButton,
    #[cfg(not(feature = "menu"))]
    _marker: std::marker::PhantomData<T>,
}

impl<T> Default for TrayAttributes<T> {
    fn default() -> Self {
        TrayAttributes {
            tooltip: None,
            icon: None,
            class_name: "Window Tray Class".to_string(),
            parent_window: None,
            #[cfg(feature = "menu")]
            menu: None,
            #[cfg(feature = "menu")]
            menu_on_button: MouseButton::Right,
            #[cfg(not(feature = "menu"))]
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> TrayAttributes<T> {
    /// Set the tooltip for the tray icon.
    pub fn with_tooltip(mut self, title: impl Into<String>) -> Self {
        self.tooltip = Some(title.into());
        self
    }

    /// Set the icon for the tray.
    pub fn with_icon(mut self, icon: Option<Icon>) -> Self {
        self.icon = icon;
        self
    }

    /// Set the class name for the tray window.
    ///
    /// WARNING: On Windows if this is the same as another window class name, it will cause issues.
    pub fn with_class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = class_name.into();
        self
    }

    /// Set the parent window for the tray icon.
    pub fn with_parent_window(mut self, parent_window: rwh_06::RawWindowHandle) -> Self {
        self.parent_window = Some(parent_window);
        self
    }

    /// Set the context menu for the tray icon.
    ///
    /// The menu will be displayed when the user clicks the tray icon with the configured button.
    /// By default, the menu opens on right-click.
    #[cfg(feature = "menu")]
    pub fn with_menu(mut self, menu: Vec<MenuEntry<T>>) -> Self {
        self.menu = Some(menu);
        self
    }

    /// Set which mouse button opens the menu.
    ///
    /// Defaults to `MouseButton::Right`.
    #[cfg(feature = "menu")]
    pub fn with_menu_on_button(mut self, button: MouseButton) -> Self {
        self.menu_on_button = button;
        self
    }
}
