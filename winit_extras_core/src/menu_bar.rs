//! Menu bar types for application menu bars.
//!
//! On macOS, this creates a global application menu bar.
//! On Windows, this creates a menu bar attached to a window.

use std::fmt;

use crate::menu::{MenuEntry, Submenu};

/// Identifier of a menu bar. Unique for each menu bar instance.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MenuBarId(usize);

impl MenuBarId {
    /// Convert the `MenuBarId` into the underlying integer.
    ///
    /// This is useful if you need to pass the ID across an FFI boundary, or store it in an atomic.
    pub const fn into_raw(self) -> usize {
        self.0
    }

    /// Construct a `MenuBarId` from the underlying integer.
    ///
    /// This should only be called with integers returned from [`MenuBarId::into_raw`].
    pub const fn from_raw(id: usize) -> Self {
        Self(id)
    }
}

impl fmt::Debug for MenuBarId {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmtr)
    }
}

/// A top-level menu in a menu bar (e.g., "File", "Edit", "View").
#[derive(Debug, Clone)]
pub struct TopLevelMenu<T> {
    /// Label displayed in the menu bar.
    pub label: String,
    /// Menu entries under this top-level menu.
    pub items: Vec<MenuEntry<T>>,
}

impl<T> TopLevelMenu<T> {
    /// Create a new top-level menu with the given label and items.
    pub fn new(label: impl Into<String>, items: Vec<MenuEntry<T>>) -> Self {
        Self {
            label: label.into(),
            items,
        }
    }
}

impl<T> From<Submenu<T>> for TopLevelMenu<T> {
    fn from(submenu: Submenu<T>) -> Self {
        Self {
            label: submenu.label,
            items: submenu.items,
        }
    }
}

/// Events emitted by menu bars.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum MenuBarEvent<T = ()> {
    /// A menu item in the menu bar was clicked.
    MenuItemClicked {
        /// The ID of the clicked menu item.
        id: T,
    },
}

/// Proxy function type for menu bar events.
pub type MenuBarProxy<T = ()> = std::sync::Arc<dyn Fn(MenuBarId, MenuBarEvent<T>) + Send + Sync>;

/// Trait for menu bar operations.
pub trait MenuBar: fmt::Debug {
    /// Get the unique identifier for this menu bar.
    fn id(&self) -> MenuBarId;

    /// Remove the menu bar.
    ///
    /// On macOS, this resets the application menu to empty.
    /// On Windows, this removes the menu bar from the window.
    fn remove(&self);
}

/// Configuration for creating a menu bar.
#[derive(Debug, Clone)]
pub struct MenuBarAttributes<T = ()> {
    /// Top-level menus in the menu bar.
    pub menus: Vec<TopLevelMenu<T>>,
    /// Parent window handle (required on Windows, ignored on macOS).
    pub parent_window: Option<rwh_06::RawWindowHandle>,
}

impl<T> Default for MenuBarAttributes<T> {
    fn default() -> Self {
        Self {
            menus: Vec::new(),
            parent_window: None,
        }
    }
}

impl<T> MenuBarAttributes<T> {
    /// Create a new menu bar configuration with the given menus.
    pub fn new(menus: Vec<TopLevelMenu<T>>) -> Self {
        Self {
            menus,
            parent_window: None,
        }
    }

    /// Set the top-level menus for the menu bar.
    pub fn with_menus(mut self, menus: Vec<TopLevelMenu<T>>) -> Self {
        self.menus = menus;
        self
    }

    /// Set the parent window for the menu bar.
    ///
    /// This is required on Windows and ignored on macOS.
    pub fn with_parent_window(mut self, parent_window: rwh_06::RawWindowHandle) -> Self {
        self.parent_window = Some(parent_window);
        self
    }
}
