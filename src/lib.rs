pub use winit_extras_core::*;

mod manager;
pub use manager::{Manager, ManagerBuilder};

#[cfg(all(feature = "menu_bar", any(target_os = "windows", target_os = "macos")))]
pub mod menu_bar;
#[cfg(all(feature = "menu_bar", any(target_os = "windows", target_os = "macos")))]
pub use menu_bar::MenuBarManager;

#[cfg(feature = "vello_renderer")]
pub use winit_extras_vello;
