//! Context menu traits.

use std::fmt;

use rwh_06::HasWindowHandle;
use winit::dpi::PhysicalPosition;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::{EventCallback, MenuEntry};

/// Handle to a live context menu popup.
///
/// Returned by [`MenuRenderer::create_menu`]. The same menu can be shown
/// multiple times at different positions. Dropping the handle closes the menu.
///
/// Not `Send`/`Sync` -- context menus are tied to the event loop thread that
/// created them.
pub trait ContextMenu: fmt::Debug {
    /// Show the menu at the given position, relative to the parent window's
    /// client area.
    ///
    /// The coordinates are converted internally to screen coordinates using
    /// the parent window handle passed to [`MenuRenderer::create_menu`].
    fn show(&self, position: PhysicalPosition<i32>);

    /// Show the menu at the given screen-space position.
    ///
    /// Use this when showing a menu in response to tray icon events, where
    /// the event position is already in screen coordinates.
    fn show_at_screen_pos(&self, position: PhysicalPosition<i32>);

    /// Close the menu if it is currently visible.
    ///
    /// Native menus dismiss automatically when an item is selected or the
    /// user clicks outside, so this is typically a no-op for them. Custom
    /// renderers (vello) use this to programmatically dismiss the popup.
    fn close(&self);

    /// Forward a window event to this menu.
    ///
    /// Used by custom-rendered menus that manage their own popup window --
    /// the application calls this from its window event handler so hover and
    /// click events reach the menu. Returns `true` if the event was consumed
    /// and should not be processed further. Native OS menus ignore this and
    /// return `false` (they receive input via the OS directly).
    fn handle_window_event(&self, _window_id: WindowId, _event: &WindowEvent) -> bool {
        false
    }
}

/// Factory trait for creating context menus.
///
/// Implementations plug a menu-rendering backend into the
/// [`Manager`][`winit_extras::Manager`]. The built-in `NativeMenuRenderer`
/// uses OS-native popup menus; `VelloMenuRenderer` (in the `winit_extras_vello`
/// crate) renders menus with vello_cpu in a custom popup window.
pub trait MenuRenderer<T: Clone + Send + Sync + 'static> {
    /// Create a context menu from the given items.
    ///
    /// The `event_loop` is passed so renderers that create popup windows
    /// (e.g. vello) can do so at menu creation time. Native renderers ignore
    /// it. The `window` parameter provides the parent for coordinate
    /// conversion when [`ContextMenu::show`] is called with client-relative
    /// coordinates.
    ///
    /// The `proxy` callback must be invoked with [`Event::MenuItemClicked`]
    /// when the user selects an item.
    ///
    /// [`Event::MenuItemClicked`]: crate::Event::MenuItemClicked
    fn create_menu(
        &self,
        event_loop: &dyn ActiveEventLoop,
        window: &dyn HasWindowHandle,
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn ContextMenu>, Box<dyn std::error::Error + Send + Sync>>;
}
