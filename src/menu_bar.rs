//! Menu bar manager for creating application menu bars.
//!
//! Provides a simple API for creating native menu bars attached to windows.

use std::marker::PhantomData;

use winit::event_loop::{EventLoop, EventLoopProxy};
use winit_tray_core::menu_bar::{
    MenuBar, MenuBarAttributes, MenuBarEvent, MenuBarId, MenuBarProxy, TopLevelMenu,
};

#[cfg(target_os = "windows")]
use winit_tray_windows::menu_bar as platform_menu_bar;

#[cfg(target_os = "macos")]
use winit_tray_macos::menu_bar as platform_menu_bar;

/// Manager for creating and handling application menu bars.
///
/// On macOS, the menu bar is a global application menu bar.
/// On Windows, the menu bar is attached to a specific window.
///
/// # Example
///
/// ```ignore
/// use winit_tray::{MenuBarManager, TopLevelMenu, MenuEntry, MenuItem};
///
/// let menu_bar_manager = MenuBarManager::new(&event_loop);
///
/// let menus = vec![
///     TopLevelMenu::new("File", vec![
///         MenuEntry::Item(MenuItem::new("new", "New")),
///         MenuEntry::Item(MenuItem::new("open", "Open")),
///         MenuEntry::Separator,
///         MenuEntry::Item(MenuItem::new("quit", "Quit")),
///     ]),
///     TopLevelMenu::new("Edit", vec![
///         MenuEntry::Item(MenuItem::new("undo", "Undo")),
///         MenuEntry::Item(MenuItem::new("redo", "Redo")),
///     ]),
/// ];
///
/// // On macOS:
/// let menu_bar = menu_bar_manager.create_menu_bar(MenuBarAttributes::new(menus))?;
///
/// // On Windows (requires a window handle):
/// let menu_bar = menu_bar_manager.create_menu_bar(
///     MenuBarAttributes::new(menus).with_parent_window(window.raw_window_handle())
/// )?;
///
/// // Handle events in your event loop
/// while let Ok((id, event)) = menu_bar_manager.try_recv() {
///     match event {
///         MenuBarEvent::MenuItemClicked { id } => println!("Clicked: {:?}", id),
///     }
/// }
/// ```
pub struct MenuBarManager<T = ()> {
    proxy: EventLoopProxy,
    receiver: std::sync::mpsc::Receiver<(MenuBarId, MenuBarEvent<T>)>,
    callback_proxy: MenuBarProxy<T>,
    _marker: PhantomData<T>,
}

impl<T> std::fmt::Debug for MenuBarManager<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBarManager")
            .field("proxy", &self.proxy)
            .field("receiver", &"<...>")
            .finish()
    }
}

impl<T: Clone + Send + Sync + 'static> MenuBarManager<T> {
    /// Create a new menu bar manager.
    pub fn new(event_loop: &EventLoop) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let proxy = event_loop.create_proxy();
        MenuBarManager {
            callback_proxy: std::sync::Arc::new({
                let sender = sender.clone();
                let proxy = proxy.clone();
                move |id, event| {
                    if let Err(e) = sender.send((id, event)) {
                        tracing::error!("Failed to send menu bar event: {e}");
                    }
                    proxy.wake_up();
                }
            }),
            proxy,
            receiver,
            _marker: PhantomData,
        }
    }

    /// Create a menu bar with the given attributes.
    ///
    /// On macOS, this sets the application's main menu.
    /// On Windows, the `parent_window` attribute must be set.
    pub fn create_menu_bar(
        &self,
        attr: MenuBarAttributes<T>,
    ) -> Result<Box<dyn MenuBar>, anyhow::Error> {
        let menu_bar = platform_menu_bar::MenuBar::new(self.callback_proxy.clone(), attr)?;
        Ok(Box::new(menu_bar))
    }

    /// Create a menu bar with the given top-level menus.
    ///
    /// This is a convenience method that creates a `MenuBarAttributes` with the given menus.
    /// On Windows, you should use `create_menu_bar` with a properly configured `MenuBarAttributes`
    /// that includes the parent window.
    #[cfg(target_os = "macos")]
    pub fn create_menu_bar_with_menus(
        &self,
        menus: Vec<TopLevelMenu<T>>,
    ) -> Result<Box<dyn MenuBar>, anyhow::Error> {
        self.create_menu_bar(MenuBarAttributes::new(menus))
    }

    /// Receive a menu bar event, blocking until one is available.
    pub fn recv(&self) -> Result<(MenuBarId, MenuBarEvent<T>), std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }

    /// Try to receive a menu bar event without blocking.
    pub fn try_recv(
        &self,
    ) -> Result<(MenuBarId, MenuBarEvent<T>), std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}
