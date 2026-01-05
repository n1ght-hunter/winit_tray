use std::marker::PhantomData;

use winit::event_loop::{EventLoop, EventLoopProxy};
pub use winit_tray_core::*;

#[cfg(target_os = "windows")]
use winit_tray_windows as platform_impl;

#[cfg(target_os = "macos")]
use winit_tray_macos as platform_impl;

#[cfg(target_os = "linux")]
use winit_tray_linux as platform_impl;

// Re-export context menu helper for regular windows
#[cfg(all(target_os = "windows", feature = "menu"))]
pub use winit_tray_windows::menu::{
    show_context_menu_for_window, show_context_menu_for_window_at_screen_pos,
};

#[cfg(all(target_os = "macos", feature = "menu"))]
pub use winit_tray_macos::menu::{
    show_context_menu_for_window, show_context_menu_for_window_at_screen_pos,
};

pub struct TrayManager<T = ()> {
    proxy: EventLoopProxy,
    receiver: std::sync::mpsc::Receiver<(tray_id::TrayId, TrayEvent<T>)>,
    callback_proxy: TrayProxy<T>,
    _marker: PhantomData<T>,
}

impl<T> std::fmt::Debug for TrayManager<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayManager")
            .field("proxy", &self.proxy)
            .field("receiver", &"<...>") // Receiver is not directly displayable
            .field("sender", &"<...>") // Sender is not directly displayable
            .finish()
    }
}

impl<T: Clone + Send + Sync + 'static> TrayManager<T> {
    pub fn new(event_loop: &EventLoop) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let proxy = event_loop.create_proxy();
        TrayManager {
            callback_proxy: std::sync::Arc::new({
                let sender = sender.clone();
                let proxy = proxy.clone();
                move |id, event| {
                    if let Err(e) = sender.send((id, event)) {
                        tracing::error!("Failed to send tray event: {e}");
                    }
                    proxy.wake_up();
                }
            }),
            proxy,
            receiver,
            _marker: PhantomData,
        }
    }

    pub fn create_tray(
        &self,
        attr: TrayAttributes<T>,
    ) -> Result<Box<dyn Tray>, anyhow::Error> {
        let tray = platform_impl::Tray::new(self.callback_proxy.clone(), attr)?;
        Ok(Box::new(tray))
    }

    pub fn recv(&self) -> Result<(tray_id::TrayId, TrayEvent<T>), std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn try_recv(
        &self,
    ) -> Result<(tray_id::TrayId, TrayEvent<T>), std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

// Popup window support
#[cfg(feature = "popup")]
pub use winit_tray_core::popup::{Popup, PopupAttributes, PopupCloseReason, PopupEvent, PopupId, PopupProxy};

/// Manager for creating and handling popup windows.
///
/// Similar to [`TrayManager`], this integrates with winit's event loop to
/// deliver popup events.
#[cfg(feature = "popup")]
pub struct PopupManager<T = ()> {
    proxy: EventLoopProxy,
    receiver: std::sync::mpsc::Receiver<(PopupId, PopupEvent<T>)>,
    callback_proxy: PopupProxy<T>,
    _marker: PhantomData<T>,
}

#[cfg(feature = "popup")]
impl<T> std::fmt::Debug for PopupManager<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopupManager")
            .field("proxy", &self.proxy)
            .field("receiver", &"<...>")
            .finish()
    }
}

#[cfg(feature = "popup")]
impl<T: Clone + Send + Sync + 'static> PopupManager<T> {
    /// Create a new popup manager.
    ///
    /// The manager integrates with the given event loop to deliver popup events.
    pub fn new(event_loop: &EventLoop) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let proxy = event_loop.create_proxy();
        PopupManager {
            callback_proxy: std::sync::Arc::new({
                let sender = sender.clone();
                let proxy = proxy.clone();
                move |id, event| {
                    if let Err(e) = sender.send((id, event)) {
                        tracing::error!("Failed to send popup event: {e}");
                    }
                    proxy.wake_up();
                }
            }),
            proxy,
            receiver,
            _marker: PhantomData,
        }
    }

    /// Create a new popup window.
    ///
    /// The popup will be displayed at the position specified in the attributes.
    #[cfg(feature = "popup")]
    pub fn create_popup(
        &self,
        attr: PopupAttributes<T>,
    ) -> Result<Box<dyn Popup>, anyhow::Error> {
        let popup = platform_impl::popup::Popup::new(self.callback_proxy.clone(), attr)?;
        Ok(Box::new(popup))
    }

    /// Try to receive a popup event without blocking.
    pub fn try_recv(
        &self,
    ) -> Result<(PopupId, PopupEvent<T>), std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    /// Receive a popup event, blocking until one is available.
    pub fn recv(&self) -> Result<(PopupId, PopupEvent<T>), std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }
}
