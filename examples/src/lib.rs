//! Common utilities for winit_tray examples.

use std::num::NonZeroU32;
use std::rc::Rc;

use winit::window::Window;

/// Softbuffer-based renderer that draws a gradient pattern.
pub struct GradientRenderer {
    #[allow(dead_code)]
    context: softbuffer::Context<Rc<Box<dyn Window>>>,
    surface: softbuffer::Surface<Rc<Box<dyn Window>>, Rc<Box<dyn Window>>>,
}

impl GradientRenderer {
    /// Create a new gradient renderer for the given window.
    pub fn new(window: Rc<Box<dyn Window>>) -> Self {
        let size = window.surface_size();
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let mut surface = softbuffer::Surface::new(&context, window).unwrap();
        surface
            .resize(
                NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
                NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
            )
            .unwrap();

        Self { context, surface }
    }

    /// Resize the rendering surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            let _ = self.surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            );
        }
    }

    /// Render a gradient pattern to the window.
    pub fn render(&mut self, width: u32, height: u32) {
        let width = width as usize;
        let height = height as usize;

        if width == 0 || height == 0 {
            return;
        }

        let mut buffer = self.surface.buffer_mut().unwrap();

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let r = (x as f32 / width as f32 * 255.0) as u8;
                let g = (y as f32 / height as f32 * 255.0) as u8;
                let b = 128;

                // Create BGR0 color for softbuffer (little-endian 0RGB format)
                buffer[idx] = u32::from_le_bytes([b, g, r, 0]);
            }
        }

        buffer.present().unwrap();
    }
}
