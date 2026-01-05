use std::marker::PhantomData;

use winit::event_loop::{EventLoop, EventLoopProxy};
use winit_tray_core::{tray_id, Tray, TrayAttributes, TrayEvent, TrayProxy};

#[cfg(target_os = "windows")]
use winit_tray_windows as platform_impl;

#[cfg(target_os = "macos")]
use winit_tray_macos as platform_impl;

#[cfg(target_os = "linux")]
use winit_tray_linux as platform_impl;

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
            .field("receiver", &"<...>")
            .field("sender", &"<...>")
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
