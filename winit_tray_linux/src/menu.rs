// Menu support for Linux tray icons using DBusMenu protocol.
//
// TODO: Implement com.canonical.dbusmenu interface
// For now, this is a placeholder. Full menu support requires implementing
// the DBusMenu specification, which is complex.
//
// See ksni's menu implementation for reference:
// https://github.com/iovxw/ksni/blob/master/src/menu.rs

#![cfg(feature = "menu")]

use winit_tray_core::MenuEntry;

pub fn create_menu<T>(_entries: &[MenuEntry<T>]) {
    // TODO: Implement DBusMenu interface
    unimplemented!("Menu support not yet implemented for Linux");
}
