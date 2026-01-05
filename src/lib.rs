pub use winit_tray_core::*;

mod tray;
pub use tray::TrayManager;

#[cfg(all(feature = "context_menu", any(target_os = "windows", target_os = "macos")))]
mod context_menu;
#[cfg(all(feature = "context_menu", any(target_os = "windows", target_os = "macos")))]
pub use context_menu::ContextMenuManager;
