use winit::event_loop::{EventLoop, EventLoopProxy};
pub use winit_tray_core::*;

#[cfg(windows)]
use winit_tray_windows as platform_impl;

pub struct TrayManager {
    proxy: EventLoopProxy,
    receiver: std::sync::mpsc::Receiver<(tray_id::TrayId, TrayEvent)>,
    callback_proxy: TrayProxy,
}

impl std::fmt::Debug for TrayManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayManager")
            .field("proxy", &self.proxy)
            .field("receiver", &"<...>") // Receiver is not directly displayable
            .field("sender", &"<...>") // Sender is not directly displayable
            .finish()
    }
}

impl TrayManager {
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
        }
    }

    pub fn create_tray(&self, attr: TrayAttributes) -> Result<Box<dyn Tray>, anyhow::Error> {
        let tray = platform_impl::Tray::new(self.callback_proxy.clone(), attr)?;
        Ok(Box::new(tray))
    }

    pub fn recv(&self) -> Result<(tray_id::TrayId, TrayEvent), std::sync::mpsc::RecvError> {
        self.receiver.recv()
    }

    pub fn try_recv(&self) -> Result<(tray_id::TrayId, TrayEvent), std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}
