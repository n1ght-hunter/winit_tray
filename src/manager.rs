use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::WindowId;
use winit_extras_core::context_menu::{ContextMenu, MenuRenderer};
use winit_extras_core::{Event, EventCallback, TrayIcon, TrayIconAttributes, TrayIconRenderer};

#[cfg(target_os = "windows")]
use winit_extras_windows::NativeTrayIconRenderer;

#[cfg(target_os = "macos")]
use winit_extras_macos::NativeTrayIconRenderer;

#[cfg(target_os = "linux")]
use winit_extras_linux::NativeTrayIconRenderer;

#[cfg(target_os = "windows")]
use winit_extras_windows::context_menu::NativeMenuRenderer as DefaultMenuRenderer;

#[cfg(target_os = "macos")]
use winit_extras_macos::context_menu::NativeMenuRenderer as DefaultMenuRenderer;

/// Entry point for tray icons and context menus.
///
/// Owns the event channel, renderers, and handles to all live menus. One
/// `Manager` handles all tray icons and context menus for the application.
///
/// The type parameter `T` is the user-defined action type carried by
/// [`Event::MenuItemClicked`]. Use `()` if you don't need menus.
///
/// # Example
///
/// ```ignore
/// let manager = Manager::new(&event_loop);
/// let icon = manager.create_tray(TrayIconAttributes::default().with_icon(icon))?;
///
/// // In proxy_wake_up:
/// while let Ok(event) = manager.try_recv() {
///     match event {
///         Event::PointerButton { .. } => { /* handle click */ }
///         Event::MenuItemClicked { id } => { /* handle menu */ }
///     }
/// }
/// ```
pub struct Manager<T: Clone + Send + Sync + 'static = ()> {
    // The EventLoopProxy is cloned into the callback, which handles all wake-ups.
    // We keep this field so the proxy lives at least as long as the Manager, in
    // case we ever need to trigger a wake from a manager method directly.
    _proxy: EventLoopProxy,
    receiver: std::sync::mpsc::Receiver<Event<T>>,
    callback: EventCallback<T>,
    tray_renderer: Box<dyn TrayIconRenderer<T>>,
    #[cfg(feature = "context_menu")]
    menu_renderer: Box<dyn MenuRenderer<T>>,
    /// Weak references to all created menus, used for auto-forwarding window
    /// events via `handle_window_event`. Dead entries are swept on each call.
    #[cfg(feature = "context_menu")]
    menus: RefCell<Vec<Weak<dyn ContextMenu>>>,
}

impl<T: Clone + Send + Sync + 'static> std::fmt::Debug for Manager<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager").finish_non_exhaustive()
    }
}

fn make_callback<T: Clone + Send + Sync + 'static>(
    sender: std::sync::mpsc::Sender<Event<T>>,
    proxy: EventLoopProxy,
) -> EventCallback<T> {
    Arc::new(move |event| {
        if let Err(e) = sender.send(event) {
            tracing::error!("Failed to send tray event: {e}");
        }
        proxy.wake_up();
    })
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl<T: Clone + Send + Sync + 'static> Manager<T> {
    /// Create a tray manager with platform-default renderers.
    pub fn new(event_loop: &EventLoop) -> Self {
        Self::builder(event_loop).build()
    }
}

/// Builder for configuring a `Manager` with custom renderers.
pub struct ManagerBuilder<T: Clone + Send + Sync + 'static> {
    event_loop_proxy: EventLoopProxy,
    sender: std::sync::mpsc::Sender<Event<T>>,
    receiver: std::sync::mpsc::Receiver<Event<T>>,
    tray_renderer: Option<Box<dyn TrayIconRenderer<T>>>,
    #[cfg(feature = "context_menu")]
    menu_renderer: Option<Box<dyn MenuRenderer<T>>>,
}

impl<T: Clone + Send + Sync + 'static> ManagerBuilder<T> {
    pub fn tray_renderer(mut self, renderer: impl TrayIconRenderer<T> + 'static) -> Self {
        self.tray_renderer = Some(Box::new(renderer));
        self
    }

    #[cfg(feature = "context_menu")]
    pub fn menu_renderer(mut self, renderer: impl MenuRenderer<T> + 'static) -> Self {
        self.menu_renderer = Some(Box::new(renderer));
        self
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub fn build(self) -> Manager<T> {
        let proxy = self.event_loop_proxy;
        let callback = make_callback(self.sender, proxy.clone());
        Manager {
            _proxy: proxy,
            receiver: self.receiver,
            callback,
            tray_renderer: self
                .tray_renderer
                .unwrap_or_else(|| Box::new(NativeTrayIconRenderer)),
            #[cfg(feature = "context_menu")]
            menu_renderer: self
                .menu_renderer
                .unwrap_or_else(|| Box::new(DefaultMenuRenderer)),
            #[cfg(feature = "context_menu")]
            menus: RefCell::new(Vec::new()),
        }
    }

    // On Linux, no default menu renderer -- must be provided or omitted
    #[cfg(target_os = "linux")]
    pub fn build(self) -> Manager<T> {
        let proxy = self.event_loop_proxy;
        let callback = make_callback(self.sender, proxy.clone());
        Manager {
            _proxy: proxy,
            receiver: self.receiver,
            callback,
            tray_renderer: self
                .tray_renderer
                .unwrap_or_else(|| Box::new(NativeTrayIconRenderer)),
            #[cfg(feature = "context_menu")]
            menu_renderer: self
                .menu_renderer
                .expect("Linux requires a menu renderer (e.g. VelloMenuRenderer). Use .menu_renderer() on the builder."),
            #[cfg(feature = "context_menu")]
            menus: RefCell::new(Vec::new()),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Manager<T> {
    /// Start building a tray manager with custom renderers.
    pub fn builder(event_loop: &EventLoop) -> ManagerBuilder<T> {
        let (sender, receiver) = std::sync::mpsc::channel();
        ManagerBuilder {
            event_loop_proxy: event_loop.create_proxy(),
            sender,
            receiver,
            tray_renderer: None,
            #[cfg(feature = "context_menu")]
            menu_renderer: None,
        }
    }

    /// Create a tray icon.
    pub fn create_tray(
        &self,
        attr: TrayIconAttributes,
    ) -> Result<Box<dyn TrayIcon>, anyhow::Error> {
        let tray = self
            .tray_renderer
            .create_tray(attr, self.callback.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(tray)
    }

    /// Create a context menu.
    ///
    /// The returned `Rc` can be stored and shown later via `show()` or
    /// `show_at_screen_pos()`. Window events are automatically forwarded to
    /// all live menus via `handle_window_event()`.
    #[cfg(feature = "context_menu")]
    pub fn create_menu(
        &self,
        event_loop: &dyn ActiveEventLoop,
        window: &impl HasWindowHandle,
        items: Vec<winit_extras_core::MenuEntry<T>>,
    ) -> Result<Rc<dyn ContextMenu>, anyhow::Error> {
        let menu = self
            .menu_renderer
            .create_menu(event_loop, window, items, self.callback.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let rc: Rc<dyn ContextMenu> = Rc::from(menu);
        self.menus.borrow_mut().push(Rc::downgrade(&rc));
        Ok(rc)
    }

    /// Forward a window event to all live context menus.
    ///
    /// Call this from `window_event()`. Returns `true` if any menu consumed the event.
    #[cfg(feature = "context_menu")]
    pub fn handle_window_event(&self, window_id: WindowId, event: &WindowEvent) -> bool {
        let mut menus = self.menus.borrow_mut();
        // Clean up dead Weak references while iterating
        menus.retain(|weak| weak.strong_count() > 0);

        for weak in menus.iter() {
            if let Some(menu) = weak.upgrade()
                && menu.handle_window_event(window_id, event)
            {
                return true;
            }
        }
        false
    }

    /// Receive an event, blocking until one is available.
    pub fn recv(&self) -> Result<Event<T>, std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&self) -> Result<Event<T>, std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}
