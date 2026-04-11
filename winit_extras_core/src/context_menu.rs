//! Context menu types.

use std::fmt;

use rwh_06::HasWindowHandle;
use winit::dpi::PhysicalPosition;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::{EventCallback, MenuEntry};

/// Handle to a context menu popup.
pub trait ContextMenu: fmt::Debug + Send + Sync {
    /// Show the context menu at the given client-relative position.
    fn show(&self, position: PhysicalPosition<i32>);

    /// Show the context menu at the given screen position.
    fn show_at_screen_pos(&self, position: PhysicalPosition<i32>);

    /// Close the context menu.
    fn close(&self);

    /// Forward a window event to this menu (for custom-rendered menus).
    ///
    /// Returns `true` if the event was consumed. Native menus return `false`.
    fn handle_window_event(&self, _window_id: WindowId, _event: &WindowEvent) -> bool {
        false
    }
}

/// Factory for creating context menus.
pub trait MenuRenderer<T: Clone + Send + Sync + 'static> {
    fn create_menu(
        &self,
        event_loop: &dyn ActiveEventLoop,
        window: &dyn HasWindowHandle,
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn ContextMenu>, Box<dyn std::error::Error + Send + Sync>>;
}
