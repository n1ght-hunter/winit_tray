//! Vello-rendered context menu implementation.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Mutex;

use rwh_06::{HasWindowHandle, RawWindowHandle};
use skrifa::FontRef;
use skrifa::MetadataProvider;
use skrifa::metrics::GlyphMetrics;
use vello_cpu::Glyph;
use vello_cpu::kurbo::Rect;
use vello_cpu::peniko::FontData;
use vello_cpu::{Pixmap, RenderContext};
use winit::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};
use winit_extras_core::context_menu::{ContextMenu as ContextMenuTrait, MenuRenderer};
use winit_extras_core::{Event, EventCallback, MenuEntry};

use crate::style::MenuStyle;

/// Renders context menus using vello_cpu + softbuffer in a custom popup window.
pub struct VelloMenuRenderer {
    style: MenuStyle,
}

impl VelloMenuRenderer {
    pub fn new() -> Self {
        Self {
            style: MenuStyle::default(),
        }
    }

    pub fn with_style(style: MenuStyle) -> Self {
        Self { style }
    }
}

impl Default for VelloMenuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send + Sync + 'static> MenuRenderer<T> for VelloMenuRenderer {
    fn create_menu(
        &self,
        event_loop: &dyn ActiveEventLoop,
        window: &dyn HasWindowHandle,
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
    ) -> Result<Box<dyn ContextMenuTrait>, Box<dyn std::error::Error + Send + Sync>> {
        let parent_handle = window.window_handle().ok().map(|h| h.as_raw());
        let menu =
            VelloContextMenu::new(event_loop, parent_handle, items, proxy, self.style.clone())
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))
                })?;
        Ok(Box::new(menu))
    }
}

/// Layout information for a single menu entry.
struct ItemLayout {
    y: u32,
    height: u32,
    is_separator: bool,
    is_enabled: bool,
}

/// Menu data (items, layout, hover state).
struct MenuData<T> {
    items: Vec<MenuEntry<T>>,
    layout: Vec<ItemLayout>,
    hover_index: Option<usize>,
    style: MenuStyle,
    menu_width: u32,
    menu_height: u32,
    proxy: EventCallback<T>,
    font_data: FontData,
}

fn load_system_font() -> FontData {
    let try_load = |path: &str| -> Option<FontData> {
        let data = std::fs::read(path).ok()?;
        Some(FontData::new(data.into(), 0))
    };

    #[cfg(target_os = "windows")]
    {
        if let Some(f) = try_load("C:\\Windows\\Fonts\\segoeui.ttf") {
            return f;
        }
    }
    #[cfg(target_os = "macos")]
    {
        for path in &[
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ] {
            if let Some(f) = try_load(path) {
                return f;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for path in &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        ] {
            if let Some(f) = try_load(path) {
                return f;
            }
        }
    }

    tracing::warn!("could not load system font, text will not render");
    FontData::new(Vec::new().into(), 0)
}

/// A context menu rendered with vello_cpu in a popup window.
pub struct VelloContextMenu<T> {
    window: Rc<Box<dyn Window>>,
    parent_handle: Option<RawWindowHandle>,
    surface: Mutex<softbuffer::Surface<Rc<Box<dyn Window>>, Rc<Box<dyn Window>>>>,
    data: Mutex<MenuData<T>>,
    renderer: Mutex<RenderContext>,
    pixmap: Mutex<Pixmap>,
}

impl<T> std::fmt::Debug for VelloContextMenu<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VelloContextMenu").finish_non_exhaustive()
    }
}

// The Window is Rc (not Send), but VelloContextMenu is only used from the
// main thread via event loop callbacks. The Mutex guards concurrent access.
unsafe impl<T: Send> Send for VelloContextMenu<T> {}
unsafe impl<T: Sync> Sync for VelloContextMenu<T> {}

impl<T: Clone + Send + Sync + 'static> VelloContextMenu<T> {
    fn new(
        event_loop: &dyn ActiveEventLoop,
        parent_handle: Option<RawWindowHandle>,
        items: Vec<MenuEntry<T>>,
        proxy: EventCallback<T>,
        style: MenuStyle,
    ) -> Result<Self, anyhow::Error> {
        // Calculate layout
        let (layout, menu_width, menu_height) = compute_layout(&items, &style);

        // Create a hidden popup window
        let attrs = WindowAttributes::default()
            .with_title("")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_surface_size(PhysicalSize::new(menu_width, menu_height));

        let window = event_loop.create_window(attrs)?;
        let window = Rc::new(window);

        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if let (Some(w), Some(h)) = (NonZeroU32::new(menu_width), NonZeroU32::new(menu_height)) {
            surface.resize(w, h).map_err(|e| anyhow::anyhow!("{e}"))?;
        }

        let font_data = load_system_font();

        let data = MenuData {
            items,
            layout,
            hover_index: None,
            style,
            menu_width,
            menu_height,
            proxy,
            font_data,
        };

        Ok(Self {
            window,
            parent_handle,
            surface: Mutex::new(surface),
            data: Mutex::new(data),
            renderer: Mutex::new(RenderContext::new(menu_width as u16, menu_height as u16)),
            pixmap: Mutex::new(Pixmap::new(menu_width as u16, menu_height as u16)),
        })
    }

    /// Returns the popup window's ID for event routing.
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Handle a window event for this popup. Returns `true` if the event was
    /// consumed (belongs to this popup window).
    pub fn handle_window_event(&self, window_id: WindowId, event: &WindowEvent) -> bool {
        if window_id != self.window.id() {
            return false;
        }

        match event {
            WindowEvent::PointerMoved { position, .. } => {
                let mut data = self.data.lock().unwrap();
                let new_hover = hit_test(&data.layout, position.y as u32);
                if new_hover != data.hover_index {
                    data.hover_index = new_hover;
                    drop(data);
                    self.render();
                    self.window.request_redraw();
                }
            }
            WindowEvent::PointerButton {
                state: ElementState::Released,
                position,
                ..
            } => {
                let data = self.data.lock().unwrap();
                if let Some(idx) = hit_test(&data.layout, position.y as u32) {
                    if data.layout[idx].is_enabled && !data.layout[idx].is_separator {
                        if let Some(id) = get_item_id(&data.items, idx) {
                            let proxy = data.proxy.clone();
                            drop(data);
                            self.window.set_visible(false);
                            (proxy)(Event::MenuItemClicked { id });
                            return true;
                        }
                    }
                }
            }
            WindowEvent::Focused(false) => {
                self.window.set_visible(false);
            }
            WindowEvent::RedrawRequested => {
                self.render();
                self.present();
            }
            _ => {}
        }

        true
    }

    fn render(&self) {
        let data = self.data.lock().unwrap();
        let mut renderer = self.renderer.lock().unwrap();
        let mut pixmap = self.pixmap.lock().unwrap();

        let style = &data.style;
        let w = data.menu_width as f64;
        let h = data.menu_height as f64;
        let hover_index = data.hover_index;

        renderer.reset();

        // Background
        renderer.set_paint(rgba(style.background));
        renderer.fill_rect(&Rect::new(0.0, 0.0, w, h));

        for (i, item_layout) in data.layout.iter().enumerate() {
            let y = item_layout.y as f64;
            let item_h = item_layout.height as f64;

            if item_layout.is_separator {
                renderer.set_paint(rgba(style.separator_color));
                let sep_y = y + item_h / 2.0;
                renderer.fill_rect(&Rect::new(8.0, sep_y, w - 8.0, sep_y + 1.0));
                continue;
            }

            let is_hovered = hover_index == Some(i) && item_layout.is_enabled;

            if is_hovered {
                renderer.set_paint(rgba(style.hover_background));
                renderer.fill_rect(&Rect::new(2.0, y, w - 2.0, y + item_h));
            }

            let text_color = if !item_layout.is_enabled {
                rgba(style.disabled_text_color)
            } else if is_hovered {
                rgba([255, 255, 255, 255])
            } else {
                rgba(style.text_color)
            };

            let label = get_item_label(&data.items, i);
            if let Some(label) = label {
                let font_size = style.font_size as f32;
                let x_offset = style.padding_x as f32;

                // Check mark
                if let Some(true) = get_item_checked(&data.items, i) {
                    renderer.set_paint(rgba(style.check_color));
                    let check_y = y as f32 + item_h as f32 / 2.0;
                    renderer.fill_rect(&Rect::new(
                        8.0,
                        (check_y - 2.0) as f64,
                        14.0,
                        (check_y + 2.0) as f64,
                    ));
                    renderer.fill_rect(&Rect::new(
                        11.0,
                        (check_y - 5.0) as f64,
                        15.0,
                        (check_y + 2.0) as f64,
                    ));
                }

                // Render text using vello_cpu glyph_run + skrifa
                renderer.set_paint(text_color);
                let glyphs = layout_text_simple(
                    &data.font_data,
                    label,
                    font_size,
                    x_offset,
                    y as f32 + item_h as f32 * 0.72, // baseline approx
                );
                if !glyphs.is_empty() {
                    renderer
                        .glyph_run(&data.font_data)
                        .font_size(font_size)
                        .fill_glyphs(glyphs.into_iter());
                }
            }
        }

        renderer.render_to_pixmap(&mut pixmap);
    }

    fn present(&self) {
        let pixmap = self.pixmap.lock().unwrap();
        let mut surface = self.surface.lock().unwrap();
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };

        let pixmap_data = pixmap.data();
        for (buffer_pixel, pixel) in buffer.iter_mut().zip(pixmap_data.iter()) {
            *buffer_pixel = u32::from_le_bytes([pixel.b, pixel.g, pixel.r, 0]);
        }

        let _ = buffer.present();
    }
}

impl<T: Clone + Send + Sync + 'static> ContextMenuTrait for VelloContextMenu<T> {
    fn show(&self, position: PhysicalPosition<i32>) {
        let screen_pos = client_to_screen(self.parent_handle, position);
        self.show_at_screen_pos(screen_pos);
    }

    fn show_at_screen_pos(&self, position: PhysicalPosition<i32>) {
        let data = self.data.lock().unwrap();
        let w = data.menu_width;
        let h = data.menu_height;
        drop(data);

        self.window.set_visible(true);
        self.window
            .set_outer_position(Position::Physical(PhysicalPosition::new(
                position.x, position.y,
            )));
        let _ = self
            .window
            .request_surface_size(Size::Physical(PhysicalSize::new(w, h)));
        self.window.focus_window();
        self.window.request_redraw();
    }

    fn close(&self) {
        self.window.set_visible(false);
    }

    fn handle_window_event(&self, window_id: WindowId, event: &WindowEvent) -> bool {
        VelloContextMenu::handle_window_event(self, window_id, event)
    }
}

fn rgba(c: [u8; 4]) -> vello_cpu::color::AlphaColor<vello_cpu::color::Srgb> {
    vello_cpu::color::AlphaColor::from_rgba8(c[0], c[1], c[2], c[3])
}

fn compute_layout<T>(items: &[MenuEntry<T>], style: &MenuStyle) -> (Vec<ItemLayout>, u32, u32) {
    let mut layout = Vec::with_capacity(items.len());
    let mut y = style.padding_y;
    let mut max_label_len = 0usize;

    for entry in items {
        match entry {
            MenuEntry::Separator => {
                layout.push(ItemLayout {
                    y,
                    height: style.separator_height,
                    is_separator: true,
                    is_enabled: false,
                });
                y += style.separator_height;
            }
            MenuEntry::Item(item) => {
                layout.push(ItemLayout {
                    y,
                    height: style.item_height,
                    is_separator: false,
                    is_enabled: item.enabled,
                });
                max_label_len = max_label_len.max(item.label.len());
                y += style.item_height;
            }
            MenuEntry::Submenu(sub) => {
                layout.push(ItemLayout {
                    y,
                    height: style.item_height,
                    is_separator: false,
                    is_enabled: sub.enabled,
                });
                let label_with_arrow = sub.label.len() + 2;
                max_label_len = max_label_len.max(label_with_arrow);
                y += style.item_height;
            }
        }
    }

    y += style.padding_y;

    let char_width = (style.font_size as f64 * 0.6) as u32;
    let text_width = (max_label_len as u32) * char_width + style.padding_x * 2;
    let width = text_width.max(style.min_width);

    (layout, width, y)
}

fn hit_test(layout: &[ItemLayout], y: u32) -> Option<usize> {
    for (i, item) in layout.iter().enumerate() {
        if y >= item.y && y < item.y + item.height && !item.is_separator {
            return Some(i);
        }
    }
    None
}

fn get_item_id<T: Clone>(items: &[MenuEntry<T>], flat_index: usize) -> Option<T> {
    items.iter().nth(flat_index).and_then(|entry| match entry {
        MenuEntry::Item(item) => Some(item.id.clone()),
        _ => None,
    })
}

fn get_item_label<T>(items: &[MenuEntry<T>], flat_index: usize) -> Option<&str> {
    items.iter().nth(flat_index).and_then(|entry| match entry {
        MenuEntry::Item(item) => Some(item.label.as_str()),
        MenuEntry::Submenu(sub) => Some(sub.label.as_str()),
        MenuEntry::Separator => None,
    })
}

fn get_item_checked<T>(items: &[MenuEntry<T>], flat_index: usize) -> Option<bool> {
    items.iter().nth(flat_index).and_then(|entry| match entry {
        MenuEntry::Item(item) => item.checked,
        _ => None,
    })
}

/// Simple text layout: map characters to positioned glyphs using skrifa's charmap
/// and glyph metrics. No shaping, no kerning -- just cmap + advance widths.
fn layout_text_simple(
    font_data: &FontData,
    text: &str,
    font_size: f32,
    x: f32,
    baseline_y: f32,
) -> Vec<Glyph> {
    let font_bytes: &[u8] = font_data.data.as_ref();
    if font_bytes.is_empty() {
        return Vec::new();
    }

    let Ok(font) = FontRef::from_index(font_bytes, 0) else {
        return Vec::new();
    };

    let charmap = font.charmap();
    let glyph_metrics = GlyphMetrics::new(
        &font,
        skrifa::instance::Size::new(font_size),
        skrifa::instance::LocationRef::default(),
    );

    let mut glyphs = Vec::with_capacity(text.len());
    let mut cx = x;

    for ch in text.chars() {
        let glyph_id = charmap.map(ch).unwrap_or_default();
        let advance = glyph_metrics
            .advance_width(glyph_id)
            .unwrap_or(font_size * 0.5);

        glyphs.push(Glyph {
            id: glyph_id.to_u32(),
            x: cx,
            y: baseline_y,
        });

        cx += advance;
    }

    glyphs
}

/// Convert client-relative coordinates to screen coordinates using the parent
/// window handle. Falls back to returning the position unchanged if the
/// platform doesn't support conversion.
fn client_to_screen(
    parent: Option<RawWindowHandle>,
    position: PhysicalPosition<i32>,
) -> PhysicalPosition<i32> {
    #[cfg(target_os = "windows")]
    if let Some(RawWindowHandle::Win32(handle)) = parent {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;

        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        let mut point = POINT {
            x: position.x,
            y: position.y,
        };
        unsafe {
            ClientToScreen(hwnd, &mut point);
        }
        return PhysicalPosition::new(point.x, point.y);
    }

    // Fallback: return as-is (caller should use show_at_screen_pos instead)
    position
}
