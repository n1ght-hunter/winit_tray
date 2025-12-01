//! Simple winit window example with basic rendering.

use std::error::Error;
use std::num::NonZeroU32;
use std::path::Path;
use std::rc::Rc;

use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::icon::{Icon, RgbaIcon};
use winit::window::{Window, WindowAttributes, WindowId};
#[cfg(feature = "menu")]
use winit_tray::{MenuEntry, MenuItem, Submenu};

fn load_icon(path: &Path) -> Result<Icon, Box<dyn Error>> {
    let image = image::open(path)?.into_rgba8();
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

struct App {
    window: Option<Rc<Box<dyn Window>>>,
    tray_manager: winit_tray::TrayManager<MenuId>,
    tray: Option<Box<dyn winit_tray::Tray>>,
    // Rendering state
    surface_context: Option<softbuffer::Context<Rc<Box<dyn Window>>>>,
    surface: Option<softbuffer::Surface<Rc<Box<dyn Window>>, Rc<Box<dyn Window>>>>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        let tray_manager = winit_tray::TrayManager::new(event_loop);
        App {
            window: None,
            tray_manager,
            tray: None,
            surface_context: None,
            surface: None,
        }
    }

    fn render(&mut self) {
        let Some(surface) = &mut self.surface else {
            return;
        };
        let Some(window) = &self.window else {
            return;
        };

        let size = window.surface_size();
        let width = size.width as usize;
        let height = size.height as usize;

        // Get buffer and draw gradient pattern
        let mut buffer = surface.buffer_mut().unwrap();

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

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Load the ferris icon
        let icon = match load_icon(Path::new("assets/ferris.png")) {
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

        // Get window size
        let size = window.surface_size();

        // Initialize softbuffer for displaying pixels
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        surface
            .resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )
            .unwrap();

        self.surface_context = Some(context);
        self.surface = Some(surface);

        // Request an initial redraw so the window appears on Wayland
        window.request_redraw();
        self.window = Some(window);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
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
                // Resize softbuffer surface
                if size.width > 0 && size.height > 0 {
                    if let Some(surface) = &mut self.surface {
                        let _ = surface.resize(
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                    }
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // Render the scene
                self.render();

                // Notify that we're done presenting
                if let Some(window) = &self.window {
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
