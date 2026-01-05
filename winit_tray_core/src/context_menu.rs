//! Context menu types for floating context menu windows.

use std::fmt;

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ButtonSource, ElementState};

use crate::menu::MenuEntry;

/// Identifier of a context menu window. Unique for each context menu window.
///
/// Whenever you receive an event specific to a context menu window, this event contains a
/// `ContextMenuWindowId` which you can then compare to the ids of your context menu windows
/// to determine which one the event is for.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextMenuWindowId(usize);

impl ContextMenuWindowId {
    /// Convert the `ContextMenuWindowId` into the underlying integer.
    ///
    /// This is useful if you need to pass the ID across an FFI boundary, or store it in an atomic.
    pub const fn into_raw(self) -> usize {
        self.0
    }

    /// Construct a `ContextMenuWindowId` from the underlying integer.
    ///
    /// This should only be called with integers returned from [`ContextMenuWindowId::into_raw`].
    pub const fn from_raw(id: usize) -> Self {
        Self(id)
    }
}

impl fmt::Debug for ContextMenuWindowId {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmtr)
    }
}

/// Reason why a context menu window was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuCloseReason {
    /// Auto-dismiss timer expired.
    Timeout,
    /// User clicked outside the context menu.
    ClickOutside,
    /// Context menu was explicitly closed via API.
    Explicit,
}

/// Events emitted by context menu windows.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum ContextMenuEvent<T = ()> {
    /// A pointer button was pressed or released.
    PointerButton {
        /// Whether the button was pressed or released.
        state: ElementState,
        /// The position of the pointer when the button was pressed (screen coordinates).
        position: PhysicalPosition<f64>,
        /// Which button was pressed.
        button: ButtonSource,
    },

    /// The context menu window was closed.
    Closed {
        /// Why the context menu was closed.
        reason: ContextMenuCloseReason,
    },

    /// A menu item in the context menu was clicked.
    MenuItemClicked {
        /// The ID of the clicked menu item.
        id: T,
    },
}

/// Proxy function type for context menu events.
pub type ContextMenuProxy<T = ()> =
    std::sync::Arc<dyn Fn(ContextMenuWindowId, ContextMenuEvent<T>) + Send + Sync>;

/// Trait for context menu window operations.
pub trait ContextMenuWindow: fmt::Debug + Send + Sync {
    /// Get the unique identifier for this context menu.
    fn id(&self) -> ContextMenuWindowId;

    /// Close the context menu window.
    fn close(&self);

    /// Move the context menu to a new position (screen coordinates).
    fn set_position(&self, position: PhysicalPosition<i32>);

    /// Resize the context menu window.
    fn set_size(&self, size: PhysicalSize<u32>);

    /// Show or hide the context menu window.
    fn set_visible(&self, visible: bool);
}

/// Configuration for creating a context menu window.
#[derive(Debug, Clone)]
pub struct ContextMenuAttributes<T = ()> {
    /// Position of the context menu in screen coordinates.
    pub position: PhysicalPosition<i32>,
    /// Size of the context menu window.
    pub size: PhysicalSize<u32>,
    /// Optional auto-dismiss timer in milliseconds. `None` means no auto-dismiss.
    pub auto_dismiss_ms: Option<u32>,
    /// Whether clicking outside the context menu closes it.
    pub close_on_click_outside: bool,
    /// Whether the context menu should stay on top of other windows.
    pub topmost: bool,
    /// Window class name (Windows-specific, ignored on other platforms).
    pub class_name: String,
    /// Optional menu shown on right-click within the context menu.
    pub menu: Option<Vec<MenuEntry<T>>>,
}

impl<T> Default for ContextMenuAttributes<T> {
    fn default() -> Self {
        Self {
            position: PhysicalPosition::new(0, 0),
            size: PhysicalSize::new(200, 100),
            auto_dismiss_ms: None,
            close_on_click_outside: true,
            topmost: true,
            class_name: "WinitContextMenu".to_string(),
            menu: None,
        }
    }
}

impl<T> ContextMenuAttributes<T> {
    /// Set the position of the context menu in screen coordinates.
    pub fn with_position(mut self, position: PhysicalPosition<i32>) -> Self {
        self.position = position;
        self
    }

    /// Set the size of the context menu window.
    pub fn with_size(mut self, size: PhysicalSize<u32>) -> Self {
        self.size = size;
        self
    }

    /// Set the auto-dismiss timer in milliseconds.
    ///
    /// The context menu will automatically close after this duration.
    /// Pass `None` to disable auto-dismiss.
    pub fn with_auto_dismiss_ms(mut self, ms: Option<u32>) -> Self {
        self.auto_dismiss_ms = ms;
        self
    }

    /// Set whether clicking outside the context menu closes it.
    pub fn with_close_on_click_outside(mut self, close: bool) -> Self {
        self.close_on_click_outside = close;
        self
    }

    /// Set whether the context menu should stay on top of other windows.
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

    /// Set the menu for the context menu.
    ///
    /// The menu will be displayed when the user right-clicks inside the context menu.
    pub fn with_menu(mut self, menu: Vec<MenuEntry<T>>) -> Self {
        self.menu = Some(menu);
        self
    }
}
