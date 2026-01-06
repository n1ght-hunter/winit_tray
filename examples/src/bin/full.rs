//! Full example demonstrating all winit_tray features:
//! - System tray icon with context menu
//! - Application menu bar
//! - Window context menu (right-click)

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

use winit_tray::{MenuEntry, MenuItem, Submenu, TrayManager};
use winit_tray_core::menu_bar::{MenuBar, MenuBarAttributes, MenuBarEvent, TopLevelMenu};

#[cfg(feature = "menu_bar")]
use winit_tray::MenuBarManager;

#[cfg(feature = "context_menu")]
use winit_tray::ContextMenuManager;

fn load_icon(path: &Path) -> Result<Icon, Box<dyn Error>> {
    let image = image::open(path)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = RgbaIcon::new(rgba, width, height)?;
    Ok(Icon::from(icon))
}

/// Actions for the system tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TrayAction {
    ShowWindow,
    HideWindow,
    Settings,
    // Theme submenu
    ThemeLight,
    ThemeDark,
    ThemeSystem,
    Exit,
}

/// Actions for the menu bar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuBarAction {
    // File menu
    New,
    Open,
    Save,
    // File > Export submenu
    ExportPng,
    ExportJpg,
    ExportPdf,
    // File > Recent submenu
    RecentFile1,
    RecentFile2,
    RecentFile3,
    ClearRecent,
    Quit,
    // Edit menu
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    // Edit > Find submenu
    Find,
    FindNext,
    FindPrevious,
    Replace,
    // View menu
    ZoomIn,
    ZoomOut,
    Fullscreen,
    // View > Panels submenu
    ShowSidebar,
    ShowToolbar,
    ShowStatusBar,
    // Help menu
    About,
    Documentation,
    CheckUpdates,
}

/// Actions for the window context menu (right-click).
#[derive(Debug, Clone, PartialEq, Eq)]
enum ContextAction {
    Refresh,
    Properties,
    Copy,
    Paste,
    // View submenu
    ViewLarge,
    ViewMedium,
    ViewSmall,
    // Sort submenu
    SortByName,
    SortByDate,
    SortBySize,
}

struct App {
    window: Option<Rc<Box<dyn Window>>>,
    renderer: Option<GradientRenderer>,
    // Tray
    tray_manager: TrayManager<TrayAction>,
    tray: Option<Box<dyn winit_tray::Tray>>,
    // Menu bar
    #[cfg(feature = "menu_bar")]
    menu_bar_manager: MenuBarManager<MenuBarAction>,
    #[cfg(feature = "menu_bar")]
    _menu_bar: Option<Box<dyn MenuBar>>,
    // Context menu
    #[cfg(feature = "context_menu")]
    context_menu_manager: ContextMenuManager,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        App {
            window: None,
            renderer: None,
            tray_manager: TrayManager::new(event_loop),
            tray: None,
            #[cfg(feature = "menu_bar")]
            menu_bar_manager: MenuBarManager::new(event_loop),
            #[cfg(feature = "menu_bar")]
            _menu_bar: None,
            #[cfg(feature = "context_menu")]
            context_menu_manager: ContextMenuManager::new(),
        }
    }

    fn build_tray_menu() -> Vec<MenuEntry<TrayAction>> {
        vec![
            MenuEntry::Item(MenuItem::new(TrayAction::ShowWindow, "Show Window")),
            MenuEntry::Item(MenuItem::new(TrayAction::HideWindow, "Hide Window")),
            MenuEntry::Separator,
            MenuEntry::Submenu(Submenu::new(
                "Theme",
                vec![
                    MenuEntry::Item(MenuItem::new(TrayAction::ThemeLight, "Light")),
                    MenuEntry::Item(MenuItem::new(TrayAction::ThemeDark, "Dark")),
                    MenuEntry::Item(MenuItem::new(TrayAction::ThemeSystem, "System Default")),
                ],
            )),
            MenuEntry::Item(MenuItem::new(TrayAction::Settings, "Settings").enabled(false)),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(TrayAction::Exit, "Exit")),
        ]
    }

    #[cfg(feature = "menu_bar")]
    fn build_menu_bar() -> Vec<TopLevelMenu<MenuBarAction>> {
        vec![
            TopLevelMenu::new(
                "File",
                vec![
                    MenuEntry::Item(MenuItem::new(MenuBarAction::New, "New")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Open, "Open...")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Save, "Save")),
                    MenuEntry::Separator,
                    MenuEntry::Submenu(Submenu::new(
                        "Export",
                        vec![
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ExportPng, "PNG Image")),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ExportJpg, "JPEG Image")),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ExportPdf, "PDF Document")),
                        ],
                    )),
                    MenuEntry::Submenu(Submenu::new(
                        "Recent Files",
                        vec![
                            MenuEntry::Item(MenuItem::new(MenuBarAction::RecentFile1, "document.txt")),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::RecentFile2, "image.png")),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::RecentFile3, "project.rs")),
                            MenuEntry::Separator,
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ClearRecent, "Clear Recent")),
                        ],
                    )),
                    MenuEntry::Separator,
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Quit, "Quit")),
                ],
            ),
            TopLevelMenu::new(
                "Edit",
                vec![
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Undo, "Undo")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Redo, "Redo")),
                    MenuEntry::Separator,
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Cut, "Cut")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Copy, "Copy")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Paste, "Paste")),
                    MenuEntry::Separator,
                    MenuEntry::Submenu(Submenu::new(
                        "Find",
                        vec![
                            MenuEntry::Item(MenuItem::new(MenuBarAction::Find, "Find...")),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::FindNext, "Find Next")),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::FindPrevious, "Find Previous")),
                            MenuEntry::Separator,
                            MenuEntry::Item(MenuItem::new(MenuBarAction::Replace, "Replace...")),
                        ],
                    )),
                ],
            ),
            TopLevelMenu::new(
                "View",
                vec![
                    MenuEntry::Item(MenuItem::new(MenuBarAction::ZoomIn, "Zoom In")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::ZoomOut, "Zoom Out")),
                    MenuEntry::Separator,
                    MenuEntry::Submenu(Submenu::new(
                        "Panels",
                        vec![
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ShowSidebar, "Sidebar").checked(true)),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ShowToolbar, "Toolbar").checked(true)),
                            MenuEntry::Item(MenuItem::new(MenuBarAction::ShowStatusBar, "Status Bar").checked(false)),
                        ],
                    )),
                    MenuEntry::Separator,
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Fullscreen, "Fullscreen")),
                ],
            ),
            TopLevelMenu::new(
                "Help",
                vec![
                    MenuEntry::Item(MenuItem::new(MenuBarAction::Documentation, "Documentation")),
                    MenuEntry::Item(MenuItem::new(MenuBarAction::CheckUpdates, "Check for Updates")),
                    MenuEntry::Separator,
                    MenuEntry::Item(MenuItem::new(MenuBarAction::About, "About")),
                ],
            ),
        ]
    }

    #[cfg(feature = "context_menu")]
    fn build_context_menu() -> Vec<MenuEntry<ContextAction>> {
        vec![
            MenuEntry::Item(MenuItem::new(ContextAction::Refresh, "Refresh")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(ContextAction::Copy, "Copy")),
            MenuEntry::Item(MenuItem::new(ContextAction::Paste, "Paste")),
            MenuEntry::Separator,
            MenuEntry::Submenu(Submenu::new(
                "View",
                vec![
                    MenuEntry::Item(MenuItem::new(ContextAction::ViewLarge, "Large Icons")),
                    MenuEntry::Item(MenuItem::new(ContextAction::ViewMedium, "Medium Icons")),
                    MenuEntry::Item(MenuItem::new(ContextAction::ViewSmall, "Small Icons")),
                ],
            )),
            MenuEntry::Submenu(Submenu::new(
                "Sort By",
                vec![
                    MenuEntry::Item(MenuItem::new(ContextAction::SortByName, "Name")),
                    MenuEntry::Item(MenuItem::new(ContextAction::SortByDate, "Date Modified")),
                    MenuEntry::Item(MenuItem::new(ContextAction::SortBySize, "Size")),
                ],
            )),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(ContextAction::Properties, "Properties")),
        ]
    }

    #[cfg(feature = "context_menu")]
    fn show_context_menu(&self, x: i32, y: i32) {
        if let Some(window) = &self.window {
            let menu = Self::build_context_menu();
            let position = winit::dpi::PhysicalPosition::new(x, y);
            if let Some(action) = self.context_menu_manager.show(window.as_ref(), &menu, position) {
                match action {
                    ContextAction::Refresh => info!("Context menu: Refresh"),
                    ContextAction::Properties => info!("Context menu: Properties"),
                    ContextAction::Copy => info!("Context menu: Copy"),
                    ContextAction::Paste => info!("Context menu: Paste"),
                    // View submenu
                    ContextAction::ViewLarge => info!("Context menu: View Large Icons"),
                    ContextAction::ViewMedium => info!("Context menu: View Medium Icons"),
                    ContextAction::ViewSmall => info!("Context menu: View Small Icons"),
                    // Sort submenu
                    ContextAction::SortByName => info!("Context menu: Sort by Name"),
                    ContextAction::SortByDate => info!("Context menu: Sort by Date"),
                    ContextAction::SortBySize => info!("Context menu: Sort by Size"),
                }
            }
        }
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Load the icon
        let icon = match load_icon(Path::new("assets/ferris.png")) {
            Ok(icon) => Some(icon),
            Err(err) => {
                warn!(%err, "failed to load icon, using default");
                None
            }
        };

        // Create the system tray with context menu
        let tray_attributes = winit_tray::TrayAttributes::default()
            .with_tooltip("Full Example - Right-click for menu")
            .with_icon(icon.clone())
            .with_menu(Self::build_tray_menu());

        self.tray = match self.tray_manager.create_tray(tray_attributes) {
            Ok(tray) => {
                info!("System tray created");
                Some(tray)
            }
            Err(err) => {
                error!(%err, "failed to create tray");
                None
            }
        };

        // Create the window
        let window_attributes = WindowAttributes::default()
            .with_window_icon(icon)
            .with_title("Full Example - All Features");

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        // Create the menu bar
        #[cfg(feature = "menu_bar")]
        {
            #[cfg(target_os = "windows")]
            let menu_bar_attrs = {
                use winit::raw_window_handle::HasWindowHandle;
                MenuBarAttributes::new(Self::build_menu_bar())
                    .with_parent_window(window.window_handle().unwrap().as_raw())
            };

            #[cfg(target_os = "macos")]
            let menu_bar_attrs = MenuBarAttributes::new(Self::build_menu_bar());

            match self.menu_bar_manager.create_menu_bar(menu_bar_attrs) {
                Ok(menu_bar) => {
                    info!("Menu bar created");
                    self._menu_bar = Some(menu_bar);
                }
                Err(err) => {
                    error!(%err, "failed to create menu bar");
                }
            }
        }

        // Initialize renderer
        self.renderer = Some(GradientRenderer::new(window.clone()));

        window.request_redraw();
        self.window = Some(window);

        info!("Application initialized with all features:");
        info!("  - System tray icon (right-click for menu)");
        info!("  - Menu bar (File, Edit, View, Help)");
        info!("  - Window context menu (right-click in window)");
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Handle tray events
        while let Ok((_id, event)) = self.tray_manager.try_recv() {
            match event {
                winit_tray::TrayEvent::PointerButton { state, button, .. } => {
                    info!(?state, ?button, "Tray icon clicked");
                }
                winit_tray::TrayEvent::MenuItemClicked { id } => {
                    info!(?id, "Tray menu item clicked");
                    match id {
                        TrayAction::ShowWindow => {
                            if let Some(window) = &self.window {
                                window.set_visible(true);
                                info!("Window shown");
                            }
                        }
                        TrayAction::HideWindow => {
                            if let Some(window) = &self.window {
                                window.set_visible(false);
                                info!("Window hidden");
                            }
                        }
                        TrayAction::Settings => {
                            info!("Settings (disabled)");
                        }
                        // Theme submenu
                        TrayAction::ThemeLight => info!("Theme: Light"),
                        TrayAction::ThemeDark => info!("Theme: Dark"),
                        TrayAction::ThemeSystem => info!("Theme: System Default"),
                        TrayAction::Exit => {
                            info!("Exiting...");
                            event_loop.exit();
                        }
                    }
                }
                _ => {}
            }
        }

        // Handle menu bar events
        #[cfg(feature = "menu_bar")]
        while let Ok((_id, event)) = self.menu_bar_manager.try_recv() {
            match event {
                MenuBarEvent::MenuItemClicked { id } => {
                    info!(?id, "Menu bar item clicked");
                    match id {
                        MenuBarAction::Quit => {
                            info!("Quit from menu bar");
                            event_loop.exit();
                        }
                        MenuBarAction::New => info!("New file"),
                        MenuBarAction::Open => info!("Open file"),
                        MenuBarAction::Save => info!("Save file"),
                        // Export submenu
                        MenuBarAction::ExportPng => info!("Export as PNG"),
                        MenuBarAction::ExportJpg => info!("Export as JPEG"),
                        MenuBarAction::ExportPdf => info!("Export as PDF"),
                        // Recent Files submenu
                        MenuBarAction::RecentFile1 => info!("Open recent: document.txt"),
                        MenuBarAction::RecentFile2 => info!("Open recent: image.png"),
                        MenuBarAction::RecentFile3 => info!("Open recent: project.rs"),
                        MenuBarAction::ClearRecent => info!("Clear recent files"),
                        // Edit menu
                        MenuBarAction::Undo => info!("Undo"),
                        MenuBarAction::Redo => info!("Redo"),
                        MenuBarAction::Cut => info!("Cut"),
                        MenuBarAction::Copy => info!("Copy"),
                        MenuBarAction::Paste => info!("Paste"),
                        // Find submenu
                        MenuBarAction::Find => info!("Find..."),
                        MenuBarAction::FindNext => info!("Find next"),
                        MenuBarAction::FindPrevious => info!("Find previous"),
                        MenuBarAction::Replace => info!("Replace..."),
                        // View menu
                        MenuBarAction::ZoomIn => info!("Zoom in"),
                        MenuBarAction::ZoomOut => info!("Zoom out"),
                        // Panels submenu
                        MenuBarAction::ShowSidebar => info!("Toggle sidebar"),
                        MenuBarAction::ShowToolbar => info!("Toggle toolbar"),
                        MenuBarAction::ShowStatusBar => info!("Toggle status bar"),
                        MenuBarAction::Fullscreen => {
                            if let Some(window) = &self.window {
                                let is_fullscreen = window.fullscreen().is_some();
                                if is_fullscreen {
                                    window.set_fullscreen(None);
                                } else {
                                    window.set_fullscreen(Some(
                                        winit::monitor::Fullscreen::Borderless(None),
                                    ));
                                }
                            }
                        }
                        // Help menu
                        MenuBarAction::About => info!("About: winit_tray full example"),
                        MenuBarAction::Documentation => info!("Opening documentation..."),
                        MenuBarAction::CheckUpdates => info!("Checking for updates..."),
                    }
                }
                _ => {}
            }
        }
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested");
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
            // Handle right-click for context menu
            WindowEvent::PointerButton {
                state: ElementState::Released,
                button,
                position,
                ..
            } => {
                if let winit::event::ButtonSource::Mouse(MouseButton::Right) = button {
                    #[cfg(feature = "context_menu")]
                    self.show_context_menu(position.x as i32, position.y as i32);
                }
            }
            _ => (),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting full example with all features...");

    let event_loop = EventLoop::new()?;
    let app = App::new(&event_loop);
    event_loop.run_app(app)?;

    Ok(())
}
