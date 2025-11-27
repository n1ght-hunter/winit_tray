//! Simple winit window example.

use std::error::Error;
use std::path::Path;

use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::icon::{Icon, RgbaIcon};
use winit::window::{Window, WindowAttributes, WindowId};
#[cfg(feature = "menu")]
use winit_tray_core::{MenuEntry, MenuItem, Submenu};

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

#[derive(Debug)]
struct App {
    window: Option<Box<dyn Window>>,
    tray_manager: winit_tray::TrayManager<MenuId>,
    tray: Option<Box<dyn winit_tray::Tray>>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        let tray_manager = winit_tray::TrayManager::new(event_loop);
        App {
            window: None,
            tray_manager,
            tray: None,
        }
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
        let tray_attributes = winit_tray_core::TrayAttributes::default()
            .with_tooltip("Winit Tray Example")
            .with_icon(icon.clone())
            .with_menu(menu);

        #[cfg(not(feature = "menu"))]
        let tray_attributes = winit_tray_core::TrayAttributes::default()
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

        let window_attributes = WindowAttributes::default().with_window_icon(icon);
        self.window = match event_loop.create_window(window_attributes) {
            Ok(window) => Some(window),
            Err(err) => {
                error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok((_id, event)) = self.tray_manager.try_recv() {
            match event {
                winit_tray_core::TrayEvent::PointerButton {
                    state,
                    position,
                    button,
                } => {
                    info!(?state, ?position, ?button, "tray icon clicked");
                }
                #[cfg(feature = "menu")]
                winit_tray_core::TrayEvent::MenuItemClicked { id } => {
                    info!(?id, "menu item clicked");
                    match id {
                        MenuAction::DarkMode => {
                            // Toggle dark mode
                            let current = winit_tray_windows::menu::is_dark_mode_enabled();
                            winit_tray_windows::menu::set_dark_mode(!current);
                            info!(dark_mode = !current, "dark mode toggled");
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
            WindowEvent::SurfaceResized(_) => {
                self.window
                    .as_ref()
                    .expect("resize event without a window")
                    .request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Redraw the application.
                //
                // It's preferable for applications that do not render continuously to render in
                // this event rather than in AboutToWait, since rendering in here allows
                // the program to gracefully handle redraws requested by the OS.

                let window = self
                    .window
                    .as_ref()
                    .expect("redraw request without a window");

                // Notify that you're about to draw.
                window.pre_present_notify();
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
