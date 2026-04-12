//! Simple winit window example with a tray icon.

use std::error::Error;
use std::path::Path;
use std::rc::Rc;

use examples::GradientRenderer;
use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::icon::{Icon, RgbaIcon};
use winit::window::{Window, WindowAttributes, WindowId};

fn load_icon(path: &Path) -> Result<Icon, Box<dyn Error>> {
    let image = image::open(path)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = RgbaIcon::new(rgba, width, height)?;
    Ok(Icon::from(icon))
}

struct App {
    window: Option<Rc<Box<dyn Window>>>,
    tray_manager: winit_extras::Manager,
    tray: Option<Box<dyn winit_extras::TrayIcon>>,
    renderer: Option<GradientRenderer>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        App {
            window: None,
            tray_manager: winit_extras::Manager::new(event_loop),
            tray: None,
            renderer: None,
        }
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let icon = match load_icon(Path::new("assets/ferris.png")) {
            Ok(icon) => Some(icon),
            Err(err) => {
                warn!(%err, "failed to load icon");
                None
            }
        };

        let mut tray_attributes =
            winit_extras::TrayIconAttributes::default().with_tooltip("Winit Tray Example");
        if let Some(icon) = icon.clone() {
            tray_attributes = tray_attributes.with_icon(icon);
        }

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

        self.renderer = Some(GradientRenderer::new(window.clone()));
        window.request_redraw();
        self.window = Some(window);
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.tray_manager.try_recv() {
            match event {
                winit_extras::Event::PointerButton {
                    state: ElementState::Released,
                    button: winit::event::ButtonSource::Mouse(MouseButton::Left),
                    ..
                } => {
                    info!("tray icon left-clicked");
                    if let Some(window) = &self.window {
                        window.focus_window();
                    }
                }
                winit_extras::Event::PointerButton { state, button, .. } => {
                    info!(?state, ?button, "tray icon clicked");
                }
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
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let size = window.surface_size();
                    renderer.render(size.width, size.height);
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
    event_loop.run_app(app)?;

    Ok(())
}
