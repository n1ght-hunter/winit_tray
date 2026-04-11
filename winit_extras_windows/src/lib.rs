#![cfg(target_os = "windows")]

pub mod msg;
mod util;

mod tray;
pub use tray::Tray;

use winit_extras_core::{EventCallback, TrayIconAttributes, TrayIconRenderer};

/// Uses native Win32 system tray APIs (`Shell_NotifyIconW`).
pub struct NativeTrayIconRenderer;

impl<T: Clone + Send + Sync + 'static> TrayIconRenderer<T> for NativeTrayIconRenderer {
    fn create_tray(
        &self,
        attributes: TrayIconAttributes,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn winit_extras_core::TrayIcon>, Box<dyn std::error::Error + Send + Sync>>
    {
        let tray = tray::Tray::new(proxy, attributes)?;
        Ok(Box::new(tray))
    }
}

#[cfg(feature = "menu")]
pub mod menu;

#[cfg(feature = "context_menu")]
pub mod context_menu;

#[cfg(feature = "menu_bar")]
pub mod menu_bar;
