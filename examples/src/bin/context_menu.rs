//! Example demonstrating right-click context menus.
//!
//! Right-click anywhere in the window to see a context menu.

use std::error::Error;
use std::num::NonZeroU32;
use std::path::Path;
use std::rc::Rc;

use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::icon::{Icon, RgbaIcon};
use winit::window::{Window, WindowAttributes, WindowId};
use winit_tray::{ContextMenuManager, MenuEntry, MenuItem, TrayManager};

fn load_icon(path: &Path) -> Result<Icon, Box<dyn Error>> {
    let image = image::open(path)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = RgbaIcon::new(rgba, width, height)?;
    Ok(Icon::from(icon))
}

/// Menu actions for the context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuAction {
    Action1,
    Action2,
    Settings,
    About,
    Exit,
}

/// Menu actions for the tray icon.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TrayMenuAction {
    ShowWindow,
    Exit,
}

struct App {
    window: Option<Rc<Box<dyn Window>>>,
    tray_manager: TrayManager<TrayMenuAction>,
    context_menu_manager: ContextMenuManager,
    tray: Option<Box<dyn winit_tray::Tray>>,
    surface_context: Option<softbuffer::Context<Rc<Box<dyn Window>>>>,
    surface: Option<softbuffer::Surface<Rc<Box<dyn Window>>, Rc<Box<dyn Window>>>>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        App {
            window: None,
            tray_manager: TrayManager::new(event_loop),
            context_menu_manager: ContextMenuManager::new(),
            tray: None,
            surface_context: None,
            surface: None,
        }
    }

    /// Show a context menu at the given position.
    fn show_context_menu(&self, event_loop: &dyn ActiveEventLoop, position: PhysicalPosition<f64>) {
        let Some(window) = self.window.as_ref() else {
            return;
        };

        let menu = vec![
            MenuEntry::Item(MenuItem::new(MenuAction::Action1, "Action 1")),
            MenuEntry::Item(MenuItem::new(MenuAction::Action2, "Action 2")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(MenuAction::Settings, "Settings")),
            MenuEntry::Item(MenuItem::new(MenuAction::About, "About")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(MenuAction::Exit, "Exit")),
        ];

        let pos = PhysicalPosition::new(position.x as i32, position.y as i32);
        if let Some(action) = self.context_menu_manager.show(window.as_ref(), &menu, pos) {
            match action {
                MenuAction::Action1 => info!("Action 1 clicked"),
                MenuAction::Action2 => info!("Action 2 clicked"),
                MenuAction::Settings => info!("Settings clicked"),
                MenuAction::About => info!("About clicked"),
                MenuAction::Exit => {
                    info!("Exit clicked");
                    event_loop.exit();
                }
            }
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

        let mut buffer = surface.buffer_mut().unwrap();

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let r = (x as f32 / width as f32 * 100.0 + 50.0) as u8;
                let g = (y as f32 / height as f32 * 100.0 + 80.0) as u8;
                let b = 150;
                buffer[idx] = u32::from_le_bytes([b, g, r, 0]);
            }
        }

        buffer.present().unwrap();
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

        let tray_menu = vec![
            MenuEntry::Item(MenuItem::new(TrayMenuAction::ShowWindow, "Show Window")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(TrayMenuAction::Exit, "Exit")),
        ];

        let tray_attributes = winit_tray::TrayAttributes::default()
            .with_tooltip("Context Menu Example")
            .with_icon(icon.clone())
            .with_menu(tray_menu);

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
            .with_title("Context Menu Example - Right-click anywhere!");

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        let size = window.surface_size();
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

        window.request_redraw();
        self.window = Some(window);

        info!("Window created. Right-click anywhere to see the context menu!");
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok((_id, event)) = self.tray_manager.try_recv() {
            match event {
                winit_tray::TrayEvent::MenuItemClicked { id } => {
                    match id {
                        TrayMenuAction::ShowWindow => {
                            if let Some(window) = &self.window {
                                window.focus_window();
                            }
                        }
                        TrayMenuAction::Exit => {
                            info!("exit from tray menu");
                            event_loop.exit();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("close requested");
                event_loop.exit();
            }
            WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => {
                if state == ElementState::Released {
                    if let winit::event::ButtonSource::Mouse(MouseButton::Right) = button {
                        self.show_context_menu(event_loop, position);
                    }
                }
            }
            WindowEvent::SurfaceResized(size) => {
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
                self.render();
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

    info!("Starting context menu example...");
    info!("Right-click anywhere in the window to see the context menu.");

    event_loop.run_app(app)?;

    Ok(())
}
