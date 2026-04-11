//! Context menu support for Windows.

use dpi::PhysicalPosition;
use rwh_06::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use winit_core::event_loop::ActiveEventLoop;
use winit_extras_core::context_menu::{ContextMenu as ContextMenuTrait, MenuRenderer};
use winit_extras_core::{Event, EventCallback, MenuEntry};

pub use crate::menu::MenuAlignment;
use crate::menu::show_context_menu_with_alignment;

pub struct ContextMenu<T> {
    hwnd: HWND,
    items: Vec<MenuEntry<T>>,
    alignment: MenuAlignment,
    proxy: EventCallback<T>,
}

impl<T> std::fmt::Debug for ContextMenu<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextMenu")
            .field("alignment", &self.alignment)
            .finish_non_exhaustive()
    }
}

unsafe impl<T: Send> Send for ContextMenu<T> {}
unsafe impl<T: Sync> Sync for ContextMenu<T> {}

impl<T: Clone + Send + Sync + 'static> ContextMenu<T> {
    pub fn new(
        window: &(impl HasWindowHandle + ?Sized),
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Self, anyhow::Error> {
        let handle = window
            .window_handle()
            .map_err(|e| anyhow::anyhow!("Failed to get window handle: {}", e))?;

        let hwnd = match handle.as_raw() {
            RawWindowHandle::Win32(win32_handle) => win32_handle.hwnd.get() as HWND,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid window handle type, expected Win32"
                ));
            }
        };

        Ok(Self {
            hwnd,
            items,
            alignment: MenuAlignment::Auto,
            proxy,
        })
    }

    pub fn with_alignment(mut self, alignment: MenuAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    fn show_at_screen_pos_internal(&self, x: i32, y: i32) {
        let result = unsafe {
            show_context_menu_with_alignment(self.hwnd, &self.items, x, y, self.alignment)
        };

        if let Some(id) = result {
            (self.proxy)(Event::MenuItemClicked { id });
        }
    }
}

impl<T: Clone + Send + Sync + 'static> ContextMenuTrait for ContextMenu<T> {
    fn show(&self, position: PhysicalPosition<i32>) {
        let mut point = POINT {
            x: position.x,
            y: position.y,
        };
        unsafe {
            ClientToScreen(self.hwnd, &mut point);
        }
        self.show_at_screen_pos_internal(point.x, point.y);
    }

    fn show_at_screen_pos(&self, position: PhysicalPosition<i32>) {
        self.show_at_screen_pos_internal(position.x, position.y);
    }

    fn close(&self) {}
}

/// Uses native Win32 popup menus (`TrackPopupMenu`).
pub struct NativeMenuRenderer;

impl<T: Clone + Send + Sync + 'static> MenuRenderer<T> for NativeMenuRenderer {
    fn create_menu(
        &self,
        _event_loop: &dyn ActiveEventLoop,
        window: &dyn HasWindowHandle,
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn ContextMenuTrait>, Box<dyn std::error::Error + Send + Sync>> {
        let menu = ContextMenu::new(window, items, proxy)?;
        Ok(Box::new(menu))
    }
}
