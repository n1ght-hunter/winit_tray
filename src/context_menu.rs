use winit::dpi::PhysicalPosition;
use winit::raw_window_handle::HasWindowHandle;
use winit_tray_core::MenuEntry;

#[cfg(target_os = "windows")]
use winit_tray_windows::context_menu as platform_context_menu;

#[cfg(target_os = "macos")]
use winit_tray_macos::context_menu as platform_context_menu;

/// Manager for showing context menus on windows.
///
/// Provides a simple API for displaying native context menus on any window.
///
/// # Example
///
/// ```ignore
/// use winit_tray::{ContextMenuManager, MenuEntry, MenuItem};
///
/// let menu_manager = ContextMenuManager::new();
///
/// let menu = vec![
///     MenuEntry::Item(MenuItem::new("action1", "Action 1")),
///     MenuEntry::Separator,
///     MenuEntry::Item(MenuItem::new("action2", "Action 2")),
/// ];
///
/// if let Some(action) = menu_manager.show(&window, &menu, position) {
///     println!("Selected: {:?}", action);
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextMenuManager;

impl ContextMenuManager {
    /// Create a new context menu manager.
    pub fn new() -> Self {
        Self
    }

    /// Show a context menu at the given position relative to a window.
    ///
    /// The position is in physical pixels relative to the window's content area (top-left origin).
    /// Returns the selected menu item's value, or `None` if the menu was dismissed.
    pub fn show<T: Clone>(
        &self,
        window: &impl HasWindowHandle,
        items: &[MenuEntry<T>],
        position: PhysicalPosition<i32>,
    ) -> Option<T> {
        platform_context_menu::show_context_menu_for_window(window, items, position)
    }

    /// Show a context menu at screen coordinates.
    ///
    /// Similar to [`Self::show`], but the position is in screen coordinates
    /// (top-left origin for Windows, converted internally for macOS).
    pub fn show_at_screen_pos<T: Clone>(
        &self,
        window: &impl HasWindowHandle,
        items: &[MenuEntry<T>],
        screen_position: PhysicalPosition<i32>,
    ) -> Option<T> {
        platform_context_menu::show_context_menu_for_window_at_screen_pos(window, items, screen_position)
    }
}
