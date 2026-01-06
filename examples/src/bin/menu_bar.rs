//! Menu bar example demonstrating application menu bars.
//!
//! On macOS, this creates a global application menu bar.
//! On Windows, this creates a menu bar attached to the window.

use std::error::Error;
use std::rc::Rc;

use examples::GradientRenderer;
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

#[cfg(feature = "menu_bar")]
use winit_tray::{MenuBarManager, MenuEntry, MenuItem};
#[cfg(feature = "menu_bar")]
use winit_tray_core::menu_bar::{MenuBar, MenuBarAttributes, MenuBarEvent, TopLevelMenu};

/// Menu item identifiers using an enum for type safety.
#[cfg(feature = "menu_bar")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuAction {
    // File menu
    New,
    Open,
    Save,
    SaveAs,
    Quit,
    // Edit menu
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    // View menu
    ZoomIn,
    ZoomOut,
    ResetZoom,
    // Help menu
    About,
    Documentation,
}

#[cfg(feature = "menu_bar")]
type MenuId = MenuAction;

#[cfg(not(feature = "menu_bar"))]
type MenuId = ();

struct App {
    window: Option<Rc<Box<dyn Window>>>,
    #[cfg(feature = "menu_bar")]
    menu_bar_manager: MenuBarManager<MenuId>,
    #[cfg(feature = "menu_bar")]
    _menu_bar: Option<Box<dyn MenuBar>>,
    renderer: Option<GradientRenderer>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        App {
            window: None,
            #[cfg(feature = "menu_bar")]
            menu_bar_manager: MenuBarManager::new(event_loop),
            #[cfg(feature = "menu_bar")]
            _menu_bar: None,
            renderer: None,
        }
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let window_attributes = WindowAttributes::default().with_title("Menu Bar Example");

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        // Create the menu bar (when menu_bar feature is enabled)
        #[cfg(feature = "menu_bar")]
        {
            let menus = vec![
                TopLevelMenu::new(
                    "File",
                    vec![
                        MenuEntry::Item(MenuItem::new(MenuAction::New, "New")),
                        MenuEntry::Item(MenuItem::new(MenuAction::Open, "Open...")),
                        MenuEntry::Separator,
                        MenuEntry::Item(MenuItem::new(MenuAction::Save, "Save")),
                        MenuEntry::Item(MenuItem::new(MenuAction::SaveAs, "Save As...")),
                        MenuEntry::Separator,
                        MenuEntry::Item(MenuItem::new(MenuAction::Quit, "Quit")),
                    ],
                ),
                TopLevelMenu::new(
                    "Edit",
                    vec![
                        MenuEntry::Item(MenuItem::new(MenuAction::Undo, "Undo")),
                        MenuEntry::Item(MenuItem::new(MenuAction::Redo, "Redo")),
                        MenuEntry::Separator,
                        MenuEntry::Item(MenuItem::new(MenuAction::Cut, "Cut")),
                        MenuEntry::Item(MenuItem::new(MenuAction::Copy, "Copy")),
                        MenuEntry::Item(MenuItem::new(MenuAction::Paste, "Paste")),
                    ],
                ),
                TopLevelMenu::new(
                    "View",
                    vec![
                        MenuEntry::Item(MenuItem::new(MenuAction::ZoomIn, "Zoom In")),
                        MenuEntry::Item(MenuItem::new(MenuAction::ZoomOut, "Zoom Out")),
                        MenuEntry::Item(MenuItem::new(MenuAction::ResetZoom, "Reset Zoom")),
                    ],
                ),
                TopLevelMenu::new(
                    "Help",
                    vec![
                        MenuEntry::Item(MenuItem::new(MenuAction::Documentation, "Documentation")),
                        MenuEntry::Separator,
                        MenuEntry::Item(MenuItem::new(MenuAction::About, "About")),
                    ],
                ),
            ];

            // On Windows, we need to provide the parent window handle
            #[cfg(target_os = "windows")]
            let menu_bar_attrs = {
                use winit::raw_window_handle::HasWindowHandle;
                MenuBarAttributes::new(menus)
                    .with_parent_window(window.window_handle().unwrap().as_raw())
            };

            // On macOS, no parent window is needed (global app menu)
            #[cfg(target_os = "macos")]
            let menu_bar_attrs = MenuBarAttributes::new(menus);

            match self.menu_bar_manager.create_menu_bar(menu_bar_attrs) {
                Ok(menu_bar) => {
                    info!("menu bar created successfully");
                    self._menu_bar = Some(menu_bar);
                }
                Err(err) => {
                    error!(%err, "failed to create menu bar");
                }
            }
        }

        // Initialize renderer
        self.renderer = Some(GradientRenderer::new(window.clone()));

        // Request an initial redraw so the window appears on Wayland
        window.request_redraw();
        self.window = Some(window);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        #[cfg(feature = "menu_bar")]
        while let Ok((_id, event)) = self.menu_bar_manager.try_recv() {
            match event {
                MenuBarEvent::MenuItemClicked { id } => {
                    info!(?id, "menu item clicked");
                    match id {
                        MenuAction::Quit => {
                            info!("quit menu item clicked, stopping");
                            event_loop.exit();
                        }
                        MenuAction::New => info!("New file"),
                        MenuAction::Open => info!("Open file dialog would appear"),
                        MenuAction::Save => info!("Saving file..."),
                        MenuAction::SaveAs => info!("Save As dialog would appear"),
                        MenuAction::Undo => info!("Undo action"),
                        MenuAction::Redo => info!("Redo action"),
                        MenuAction::Cut => info!("Cut to clipboard"),
                        MenuAction::Copy => info!("Copy to clipboard"),
                        MenuAction::Paste => info!("Paste from clipboard"),
                        MenuAction::ZoomIn => info!("Zooming in..."),
                        MenuAction::ZoomOut => info!("Zooming out..."),
                        MenuAction::ResetZoom => info!("Zoom reset to 100%"),
                        MenuAction::About => info!("About dialog would appear"),
                        MenuAction::Documentation => info!("Opening documentation..."),
                    }
                }
                _ => {} // Handle future event types
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

    #[cfg(not(feature = "menu_bar"))]
    {
        eprintln!("This example requires the 'menu_bar' feature.");
        eprintln!("Run with: cargo run --example menu_bar --features menu_bar");
        return Ok(());
    }

    #[cfg(feature = "menu_bar")]
    {
        let event_loop = EventLoop::new()?;
        let app = App::new(&event_loop);
        event_loop.run_app(app)?;
        Ok(())
    }
}
