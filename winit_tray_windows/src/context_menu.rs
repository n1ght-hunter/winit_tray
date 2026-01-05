//! Context menu support for Windows windows.
//!
//! This module provides context menu functionality for windows (not tray icons).
//! For tray icon menus, see the `menu` module.

pub use crate::menu::{
    show_context_menu_for_window, show_context_menu_for_window_at_screen_pos, MenuAlignment,
};
