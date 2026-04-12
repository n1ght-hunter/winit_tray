//! Vello-based context menu renderer for `winit_extras`.
//!
//! Renders context menus using `vello_cpu` and `softbuffer` in a custom popup
//! window. Works on all platforms and is the recommended renderer on Linux
//! where no native popup menu API exists.
//!
//! # Usage
//!
//! ```ignore
//! use winit_extras::Manager;
//! use winit_extras_vello::VelloMenuRenderer;
//!
//! let manager = Manager::builder(&event_loop)
//!     .menu_renderer(VelloMenuRenderer::new())
//!     .build();
//! ```

mod menu;
mod style;

pub use menu::{VelloContextMenu, VelloMenuRenderer};
pub use style::MenuStyle;
