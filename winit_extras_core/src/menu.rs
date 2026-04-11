//! Menu types for tray context menus.

use winit::icon::Icon;

/// A clickable menu item with a generic ID type.
#[derive(Debug, Clone)]
pub struct MenuItem<T> {
    /// Unique identifier for this menu item.
    pub id: T,
    /// Text label displayed for this item.
    pub label: String,
    /// Whether this item is enabled (clickable).
    pub enabled: bool,
    /// Check state: `None` = not checkable, `Some(bool)` = checkable with state.
    pub checked: Option<bool>,
    /// Optional icon displayed next to the label.
    pub icon: Option<Icon>,
}

impl<T> MenuItem<T> {
    /// Create a new menu item with the given ID and label.
    pub fn new(id: T, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
            checked: None,
            icon: None,
        }
    }

    /// Set whether this item is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Make this item checkable with the given initial state.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Set an icon for this menu item.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// A submenu containing nested menu entries.
#[derive(Debug, Clone)]
pub struct Submenu<T> {
    /// Text label displayed for this submenu.
    pub label: String,
    /// Whether this submenu is enabled.
    pub enabled: bool,
    /// Nested menu entries.
    pub items: Vec<MenuEntry<T>>,
}

impl<T> Submenu<T> {
    /// Create a new submenu with the given label and items.
    pub fn new(label: impl Into<String>, items: Vec<MenuEntry<T>>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            items,
        }
    }

    /// Set whether this submenu is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// An entry in a menu, which can be an item, submenu, or separator.
#[derive(Debug, Clone)]
pub enum MenuEntry<T> {
    /// A clickable menu item.
    Item(MenuItem<T>),
    /// A submenu with nested entries.
    Submenu(Submenu<T>),
    /// A visual separator line.
    Separator,
}
