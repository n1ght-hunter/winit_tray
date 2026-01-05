//! Popup window types for floating windows.

use std::fmt;

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ButtonSource, ElementState};

#[cfg(feature = "menu")]
use crate::menu::MenuEntry;

/// Identifier of a popup window. Unique for each popup window.
///
/// Whenever you receive an event specific to a popup window, this event contains a `PopupId` which
/// you can then compare to the ids of your popup windows to determine which one the event is for.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PopupId(usize);

impl PopupId {
    /// Convert the `PopupId` into the underlying integer.
    ///
    /// This is useful if you need to pass the ID across an FFI boundary, or store it in an atomic.
    pub const fn into_raw(self) -> usize {
        self.0
    }

    /// Construct a `PopupId` from the underlying integer.
    ///
    /// This should only be called with integers returned from [`PopupId::into_raw`].
    pub const fn from_raw(id: usize) -> Self {
        Self(id)
    }
}

impl fmt::Debug for PopupId {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmtr)
    }
}

/// Reason why a popup window was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupCloseReason {
    /// Auto-dismiss timer expired.
    Timeout,
    /// User clicked outside the popup.
    ClickOutside,
    /// Popup was explicitly closed via API.
    Explicit,
}

/// Events emitted by popup windows.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum PopupEvent<T = ()> {
    /// A pointer button was pressed or released.
    PointerButton {
        /// Whether the button was pressed or released.
        state: ElementState,
        /// The position of the pointer when the button was pressed (screen coordinates).
        position: PhysicalPosition<f64>,
        /// Which button was pressed.
        button: ButtonSource,
    },

    /// The popup window was closed.
    Closed {
        /// Why the popup was closed.
        reason: PopupCloseReason,
    },

    /// A menu item in the popup's context menu was clicked.
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

/// Proxy function type for popup events.
pub type PopupProxy<T = ()> = std::sync::Arc<dyn Fn(PopupId, PopupEvent<T>) + Send + Sync>;

/// Trait for popup window operations.
pub trait Popup: fmt::Debug + Send + Sync {
    /// Get the unique identifier for this popup.
    fn id(&self) -> PopupId;

    /// Close the popup window.
    fn close(&self);

    /// Move the popup to a new position (screen coordinates).
    fn set_position(&self, position: PhysicalPosition<i32>);

    /// Resize the popup window.
    fn set_size(&self, size: PhysicalSize<u32>);

    /// Show or hide the popup window.
    fn set_visible(&self, visible: bool);
}

/// Configuration for creating a popup window.
#[derive(Debug, Clone)]
pub struct PopupAttributes<T = ()> {
    /// Position of the popup in screen coordinates.
    pub position: PhysicalPosition<i32>,
    /// Size of the popup window.
    pub size: PhysicalSize<u32>,
    /// Optional auto-dismiss timer in milliseconds. `None` means no auto-dismiss.
    pub auto_dismiss_ms: Option<u32>,
    /// Whether clicking outside the popup closes it.
    pub close_on_click_outside: bool,
    /// Whether the popup should stay on top of other windows.
    pub topmost: bool,
    /// Window class name (Windows-specific, ignored on other platforms).
    pub class_name: String,
    /// Optional context menu shown on right-click within the popup.
    #[cfg(feature = "menu")]
    pub menu: Option<Vec<MenuEntry<T>>>,
    /// Phantom marker for when menu feature is disabled.
    #[cfg(not(feature = "menu"))]
    _marker: std::marker::PhantomData<T>,
}

impl<T> Default for PopupAttributes<T> {
    fn default() -> Self {
        Self {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(200, 100),
            auto_dismiss_ms: None,
            close_on_click_outside: true,
            topmost: true,
            class_name: "WinitPopup".to_string(),
            #[cfg(feature = "menu")]
            menu: None,
            #[cfg(not(feature = "menu"))]
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> PopupAttributes<T> {
    /// Set the position of the popup in screen coordinates.
    pub fn with_position(mut self, position: PhysicalPosition<i32>) -> Self {
        self.position = position;
        self
    }

    /// Set the size of the popup window.
    pub fn with_size(mut self, size: PhysicalSize<u32>) -> Self {
        self.size = size;
        self
    }

    /// Set the auto-dismiss timer in milliseconds.
    ///
    /// The popup will automatically close after this duration.
    /// Pass `None` to disable auto-dismiss.
    pub fn with_auto_dismiss_ms(mut self, ms: Option<u32>) -> Self {
        self.auto_dismiss_ms = ms;
        self
    }

    /// Set whether clicking outside the popup closes it.
    pub fn with_close_on_click_outside(mut self, close: bool) -> Self {
        self.close_on_click_outside = close;
        self
    }

    /// Set whether the popup should stay on top of other windows.
    pub fn with_topmost(mut self, topmost: bool) -> Self {
        self.topmost = topmost;
        self
    }

    /// Set the window class name (Windows-specific).
    ///
    /// WARNING: On Windows, if this is the same as another window class name, it will cause issues.
    pub fn with_class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = class_name.into();
        self
    }

    /// Set the context menu for the popup.
    ///
    /// The menu will be displayed when the user right-clicks inside the popup.
    #[cfg(feature = "menu")]
    pub fn with_menu(mut self, menu: Vec<MenuEntry<T>>) -> Self {
        self.menu = Some(menu);
        self
    }
}
