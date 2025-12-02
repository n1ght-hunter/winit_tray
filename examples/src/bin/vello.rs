//! Simple winit window example with basic rendering.

use std::error::Error;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

use tracing::{error, info, warn};
use vello_cpu::color::palette::css::WHITE;
use vello_cpu::kurbo::{Affine, Rect};
use vello_cpu::peniko::ImageSampler;
use vello_cpu::{Image, ImageSource, Pixmap, RenderContext};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::icon::{Icon, RgbaIcon};
use winit::window::{Window, WindowAttributes, WindowId};
#[cfg(feature = "menu")]
use winit_tray::{MenuEntry, MenuItem, Submenu};

static FERRIS_PNG: &[u8] = include_bytes!("../../../assets/ferris.png");

fn load_icon() -> Result<Icon, Box<dyn Error>> {
    let image = image::load_from_memory(FERRIS_PNG)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = RgbaIcon::new(rgba, width, height)?;
    Ok(Icon::from(icon))
}

/// Menu item identifiers using an enum for type safety.
#[cfg(feature = "menu")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuAction {
    Open,
    Settings,
    DarkMode,
    OptionA,
    OptionB,
    Exit,
}

#[cfg(feature = "menu")]
type MenuId = MenuAction;

#[cfg(not(feature = "menu"))]
type MenuId = ();

enum RenderState {
    Uninitialized,
    Ready {
        window: Rc<Box<dyn Window>>,
        surface: softbuffer::Surface<Rc<Box<dyn Window>>, Rc<Box<dyn Window>>>,
    },
}

struct App {
    state: RenderState,
    tray_manager: winit_tray::TrayManager<MenuId>,
    tray: Option<Box<dyn winit_tray::Tray>>,
    renderer: RenderContext,
    pixmap: Pixmap,
    bg_image: ImageSource,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        let tray_manager = winit_tray::TrayManager::new(event_loop);
        let width = 800;
        let height = 600;
        App {
            state: RenderState::Uninitialized,
            tray_manager,
            tray: None,
            renderer: RenderContext::new(width, height),
            pixmap: Pixmap::new(width, height),
            bg_image: ImageSource::Pixmap(Arc::new(Pixmap::from_png(FERRIS_PNG).unwrap())),
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if let RenderState::Ready { surface, .. } = &mut self.state {
            surface
                .resize(
                    NonZeroU32::new(size.width).unwrap(),
                    NonZeroU32::new(size.height).unwrap(),
                )
                .unwrap();

            self.renderer = RenderContext::new(size.width as u16, size.height as u16);
        }
    }

    fn render(&mut self) {
        let RenderState::Ready {
            window, surface, ..
        } = &mut self.state
        else {
            return;
        };

        let size = window.surface_size();
        let width = size.width as usize;
        let height = size.height as usize;

        self.renderer.reset();
        let bg = Rect::new(0.0, 0.0, width as f64, height as f64);
        self.renderer.set_paint(WHITE);
        self.renderer.fill_rect(&bg);

        // Get the image dimensions
        let (img_width, img_height) = match &self.bg_image {
            ImageSource::Pixmap(p) => (p.width() as f64, p.height() as f64),
            _ => (640.0, 480.0), // fallback dimensions
        };

        // Calculate scale to fit the image within the window while maintaining aspect ratio
        let scale_x = width as f64 / img_width;
        let scale_y = height as f64 / img_height;
        let scale = scale_x.min(scale_y);

        // Calculate the scaled dimensions
        let scaled_width = img_width * scale;
        let scaled_height = img_height * scale;

        // Center the image in the window
        let offset_x = (width as f64 - scaled_width) / 2.0;
        let offset_y = (height as f64 - scaled_height) / 2.0;

        self.renderer.set_transform(Affine::translate((offset_x, offset_y)) * Affine::scale(scale));
        self.renderer.set_paint(Image {
            image: self.bg_image.clone(),
            sampler: ImageSampler::default(),
        });
        self.renderer.fill_rect(&Rect::new(0.0, 0.0, img_width, img_height));

        self.renderer.render_to_pixmap(&mut self.pixmap);

        let mut buffer = surface.buffer_mut().unwrap();
        let pixmap_data = self.pixmap.data();

        // Convert RGBA to BGRA/XRGB format expected by softbuffer
        for (buffer_pixel, pixel) in buffer.iter_mut().zip(pixmap_data.iter()) {
            // softbuffer expects 0RGB format (little-endian: B, G, R, 0)
            // Our pixmap is premultiplied RGBA
            *buffer_pixel = u32::from_le_bytes([pixel.b, pixel.g, pixel.r, 0]);
        }

        buffer.present().unwrap();
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Load the ferris icon
        let icon = match load_icon() {
            Ok(icon) => Some(icon),
            Err(err) => {
                warn!(%err, "failed to load icon");
                None
            }
        };

        // Build the context menu (when menu feature is enabled)
        #[cfg(feature = "menu")]
        let menu = vec![
            MenuEntry::Item(MenuItem::new(MenuAction::Open, "Open")),
            MenuEntry::Item(MenuItem::new(MenuAction::Settings, "Settings").enabled(false)),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(MenuAction::DarkMode, "Dark Mode").checked(false)),
            MenuEntry::Submenu(Submenu::new(
                "Options",
                vec![
                    MenuEntry::Item(MenuItem::new(MenuAction::OptionA, "Option A").checked(true)),
                    MenuEntry::Item(MenuItem::new(MenuAction::OptionB, "Option B").checked(false)),
                ],
            )),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(MenuAction::Exit, "Exit")),
        ];

        #[cfg(feature = "menu")]
        let tray_attributes = winit_tray::TrayAttributes::default()
            .with_tooltip("Winit Tray Example")
            .with_icon(icon.clone())
            .with_menu(menu);

        #[cfg(not(feature = "menu"))]
        let tray_attributes = winit_tray::TrayAttributes::default()
            .with_tooltip("Winit Tray Example")
            .with_icon(icon.clone());

        self.tray = match self.tray_manager.create_tray(tray_attributes) {
            Ok(tray) => Some(tray),
            Err(err) => {
                error!(%err, "failed to create tray");
                event_loop.exit();
                return;
            }
        };

        let window_attributes = WindowAttributes::default()
            .with_window_icon(icon)
            .with_title("Winit Tray Example");

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        // Initialize softbuffer for displaying pixels
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        surface
            .resize(
                NonZeroU32::new(self.pixmap.width() as u32).unwrap(),
                NonZeroU32::new(self.pixmap.height() as u32).unwrap(),
            )
            .unwrap();

        // Request an initial redraw so the window appears on Wayland
        window.request_redraw();
        self.state = RenderState::Ready { window, surface };
    }

    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
        while let Ok((_id, event)) = self.tray_manager.try_recv() {
            match event {
                winit_tray::TrayEvent::PointerButton {
                    state,
                    position,
                    button,
                } => {
                    info!(?state, ?position, ?button, "tray icon clicked");
                }
                #[cfg(feature = "menu")]
                winit_tray::TrayEvent::MenuItemClicked { id } => {
                    info!(?id, "menu item clicked");
                    match id {
                        MenuAction::DarkMode => {
                            // Toggle dark mode (Windows only)
                            #[cfg(target_os = "windows")]
                            {
                                let current = winit_tray_windows::menu::is_dark_mode_enabled();
                                winit_tray_windows::menu::set_dark_mode(!current);
                                info!(dark_mode = !current, "dark mode toggled");
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                info!("dark mode toggle not implemented on this platform");
                            }
                        }
                        MenuAction::Exit => {
                            info!("exit menu item clicked, stopping");
                            event_loop.exit();
                        }
                        MenuAction::Open => {
                            info!("open clicked");
                        }
                        MenuAction::OptionA | MenuAction::OptionB => {
                            info!(?id, "option selected");
                        }
                        _ => {}
                    }
                }
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("close requested, stopping");
                event_loop.exit();
            }
            WindowEvent::SurfaceResized(size) => {
                self.resize(size);

                if let RenderState::Ready { window, .. } = &self.state {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // Render the scene
                self.render();

                // Notify that we're done presenting
                if let RenderState::Ready { window, .. } = &self.state {
                    window.pre_present_notify();
                }
            }
            _ => (),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let event_loop = EventLoop::new()?;

    let app = App::new(&event_loop);

    // For alternative loop run options see `pump_events` and `run_on_demand` examples.
    event_loop.run_app(app)?;

    Ok(())
}
