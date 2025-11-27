use std::marker::PhantomData;

use winit::{
    dpi::PhysicalPosition,
    event::{ButtonSource, ElementState},
    icon::Icon,
};

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

        // /// Indicates whether the event is created by a primary pointer.
        // ///
        // /// A pointer is considered primary when it's a mouse, the first finger in a multi-touch
        // /// interaction, or an unknown pointer source.
        // primary: bool,
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
    __Phantom(PhantomData<T>),
}

pub type TrayProxy<T = ()> = std::sync::Arc<dyn Fn(tray_id::TrayId, TrayEvent<T>) + Send + Sync>;

pub trait Tray: Send + Sync + std::fmt::Debug {
    fn id(&self) -> tray_id::TrayId;
}

#[derive(Debug)]
pub struct TrayAttributes<T = ()> {
    pub tooltip: Option<String>,
    pub class_name: String,
    pub icon: Option<Icon>,
    pub(crate) parent_window: Option<SendSyncRawWindowHandle>,
    #[cfg(feature = "menu")]
    pub menu: Option<Vec<MenuEntry<T>>>,
    #[cfg(not(feature = "menu"))]
    _marker: PhantomData<T>,
}

impl<T> Default for TrayAttributes<T> {
    fn default() -> Self {
        TrayAttributes {
            tooltip: None,
            icon: None,
            parent_window: None,
            class_name: "Window Tray Class".to_string(),
            #[cfg(feature = "menu")]
            menu: None,
            #[cfg(not(feature = "menu"))]
            _marker: PhantomData,
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

    /// Build window with parent window.
    ///
    /// The default is `None`.
    ///
    /// ## Safety
    ///
    /// `parent_window` must be a valid window handle.
    ///
    /// ## Platform-specific
    ///
    /// - **Windows** : A child window has the WS_CHILD style and is confined
    ///   to the client area of its parent window. For more information, see
    ///   <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#child-windows>
    /// - **X11**: A child window is confined to the client area of its parent window.
    /// - **Android / iOS / Wayland / Web:** Unsupported.
    #[inline]
    pub unsafe fn with_parent_window(
        mut self,
        parent_window: Option<rwh_06::RawWindowHandle>,
    ) -> Self {
        self.parent_window = parent_window.map(SendSyncRawWindowHandle);
        self
    }

    /// Get the parent window stored on the attributes.
    pub fn parent_window(&self) -> Option<&rwh_06::RawWindowHandle> {
        self.parent_window.as_ref().map(|handle| &handle.0)
    }

    /// Set the context menu for the tray icon.
    ///
    /// The menu will be displayed when the user right-clicks the tray icon.
    #[cfg(feature = "menu")]
    pub fn with_menu(mut self, menu: Vec<MenuEntry<T>>) -> Self {
        self.menu = Some(menu);
        self
    }
}

/// Wrapper for [`rwh_06::RawWindowHandle`] for [`WindowAttributes::parent_window`].
///
/// # Safety
///
/// The user has to account for that when using [`WindowAttributes::with_parent_window()`],
/// which is `unsafe`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SendSyncRawWindowHandle(pub(crate) rwh_06::RawWindowHandle);

unsafe impl Send for SendSyncRawWindowHandle {}
unsafe impl Sync for SendSyncRawWindowHandle {}
