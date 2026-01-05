//! Example demonstrating right-click context menus and popup windows.
//!
//! Right-click anywhere in the main window to see a context menu.
//! Select "Show Popup" to create a floating popup window.

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
use winit_tray::{MenuEntry, MenuItem, PopupAttributes, PopupManager, TrayManager};

fn load_icon(path: &Path) -> Result<Icon, Box<dyn Error>> {
    let image = image::open(path)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = RgbaIcon::new(rgba, width, height)?;
    Ok(Icon::from(icon))
}

/// Menu actions for the main window context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowMenuAction {
    ShowPopup,
    Settings,
    About,
    Exit,
}

/// Menu actions for the popup window context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PopupMenuAction {
    DoSomething,
    AnotherAction,
    ClosePopup,
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
    popup_manager: PopupManager<PopupMenuAction>,
    tray: Option<Box<dyn winit_tray::Tray>>,
    popup: Option<Box<dyn winit_tray::Popup>>,
    // Track last click position for context menu
    last_click_position: Option<PhysicalPosition<f64>>,
    // Rendering state
    surface_context: Option<softbuffer::Context<Rc<Box<dyn Window>>>>,
    surface: Option<softbuffer::Surface<Rc<Box<dyn Window>>, Rc<Box<dyn Window>>>>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        App {
            window: None,
            tray_manager: TrayManager::new(event_loop),
            popup_manager: PopupManager::new(event_loop),
            tray: None,
            popup: None,
            last_click_position: None,
            surface_context: None,
            surface: None,
        }
    }

    /// Show a context menu at the given position.
    fn show_window_context_menu(&mut self, position: PhysicalPosition<f64>) {
        let Some(window) = &self.window else {
            return;
        };

        let menu = vec![
            MenuEntry::Item(MenuItem::new(WindowMenuAction::ShowPopup, "Show Popup")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(WindowMenuAction::Settings, "Settings")),
            MenuEntry::Item(MenuItem::new(WindowMenuAction::About, "About")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(WindowMenuAction::Exit, "Exit")),
        ];

        // Use the context menu helper to show the menu
        let client_pos = PhysicalPosition::new(position.x as i32, position.y as i32);
        if let Some(action) =
            winit_tray::show_context_menu_for_window(window.as_ref(), &menu, client_pos)
        {
            self.handle_window_menu_action(action);
        }
    }

    /// Handle a window context menu action.
    fn handle_window_menu_action(&mut self, action: WindowMenuAction) {
        match action {
            WindowMenuAction::ShowPopup => {
                if let Some(pos) = self.last_click_position {
                    self.create_popup_at(pos);
                }
            }
            WindowMenuAction::Settings => {
                info!("Settings clicked");
            }
            WindowMenuAction::About => {
                info!("About clicked");
            }
            WindowMenuAction::Exit => {
                info!("Exit clicked from window menu");
                // We can't exit directly here, but we'll handle it in the event loop
            }
        }
    }

    /// Create a popup window at the given screen position.
    fn create_popup_at(&mut self, position: PhysicalPosition<f64>) {
        // Close existing popup if any
        if let Some(popup) = self.popup.take() {
            popup.close();
        }

        // Get window position to convert to screen coordinates
        let screen_pos = if let Some(window) = &self.window {
            if let Ok(outer_pos) = window.outer_position() {
                PhysicalPosition::new(
                    outer_pos.x + position.x as i32,
                    outer_pos.y + position.y as i32 + 30, // Add offset for title bar
                )
            } else {
                PhysicalPosition::new(position.x as i32, position.y as i32)
            }
        } else {
            PhysicalPosition::new(position.x as i32, position.y as i32)
        };

        // Create popup menu
        let popup_menu = vec![
            MenuEntry::Item(MenuItem::new(PopupMenuAction::DoSomething, "Do Something")),
            MenuEntry::Item(MenuItem::new(PopupMenuAction::AnotherAction, "Another Action")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(PopupMenuAction::ClosePopup, "Close Popup")),
        ];

        let attr = PopupAttributes::default()
            .with_position(screen_pos)
            .with_size(winit::dpi::PhysicalSize::new(200, 150))
            .with_auto_dismiss_ms(Some(10000)) // Auto-close after 10 seconds
            .with_close_on_click_outside(true)
            .with_menu(popup_menu);

        match self.popup_manager.create_popup(attr) {
            Ok(popup) => {
                info!(?screen_pos, "Popup created");
                self.popup = Some(popup);
            }
            Err(e) => {
                error!(%e, "Failed to create popup");
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

        // Draw a nice gradient with a hint about right-clicking
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
        // Load the ferris icon
        let icon = match load_icon(Path::new("assets/ferris.png")) {
            Ok(icon) => Some(icon),
            Err(err) => {
                warn!(%err, "failed to load icon");
                None
            }
        };

        // Build tray menu
        let tray_menu = vec![
            MenuEntry::Item(MenuItem::new(TrayMenuAction::ShowWindow, "Show Window")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(TrayMenuAction::Exit, "Exit")),
        ];

        let tray_attributes = winit_tray::TrayAttributes::default()
            .with_tooltip("Popup Menu Example")
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
            .with_title("Popup Menu Example - Right-click anywhere!");

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
        // Handle tray events
        while let Ok((_id, event)) = self.tray_manager.try_recv() {
            match event {
                winit_tray::TrayEvent::PointerButton {
                    state,
                    position,
                    button,
                } => {
                    info!(?state, ?position, ?button, "tray icon clicked");
                }
                winit_tray::TrayEvent::MenuItemClicked { id } => {
                    info!(?id, "tray menu item clicked");
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

        // Handle popup events
        while let Ok((id, event)) = self.popup_manager.try_recv() {
            match event {
                winit_tray::PopupEvent::Closed { reason } => {
                    info!(?id, ?reason, "popup closed");
                    self.popup = None;
                }
                winit_tray::PopupEvent::PointerButton {
                    state,
                    position,
                    button,
                } => {
                    info!(?state, ?position, ?button, "popup clicked");
                }
                winit_tray::PopupEvent::MenuItemClicked { id: menu_id } => {
                    info!(?menu_id, "popup menu item clicked");
                    match menu_id {
                        PopupMenuAction::DoSomething => {
                            info!("Do Something clicked in popup");
                        }
                        PopupMenuAction::AnotherAction => {
                            info!("Another Action clicked in popup");
                        }
                        PopupMenuAction::ClosePopup => {
                            if let Some(popup) = &self.popup {
                                popup.close();
                            }
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
                info!("close requested, stopping");
                event_loop.exit();
            }
            WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => {
                // Track click position for popup creation
                if state == ElementState::Pressed {
                    self.last_click_position = Some(position);
                }

                // Show context menu on right-click release
                if state == ElementState::Released {
                    if let winit::event::ButtonSource::Mouse(MouseButton::Right) = button {
                        self.show_window_context_menu(position);
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

    info!("Starting popup menu example...");
    info!("Right-click anywhere in the window to see the context menu.");
    info!("Select 'Show Popup' to create a floating popup window.");

    event_loop.run_app(app)?;

    Ok(())
}
