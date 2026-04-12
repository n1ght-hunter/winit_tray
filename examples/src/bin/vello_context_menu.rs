//! Example demonstrating vello-rendered context menus on both windows and tray icons.

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
use winit_extras::context_menu::ContextMenu;
use winit_extras::{Event, Manager, MenuEntry, MenuItem};
use winit_extras_vello::VelloMenuRenderer;

type WindowHandle = Rc<Box<dyn Window>>;
type SoftbufferSurface = softbuffer::Surface<WindowHandle, WindowHandle>;

fn load_icon(path: &Path) -> Result<Icon, Box<dyn Error>> {
    let image = image::open(path)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    let icon = RgbaIcon::new(rgba, width, height)?;
    Ok(Icon::from(icon))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    ShowWindow,
    Action1,
    Action2,
    Settings,
    About,
    Exit,
}

struct App {
    window: Option<WindowHandle>,
    tray: Manager<Action>,
    tray_icon: Option<Box<dyn winit_extras::TrayIcon>>,
    window_menu: Option<Rc<dyn ContextMenu>>,
    tray_menu: Option<Rc<dyn ContextMenu>>,
    surface: Option<SoftbufferSurface>,
}

impl App {
    fn new(event_loop: &EventLoop) -> Self {
        App {
            window: None,
            tray: Manager::builder(event_loop)
                .menu_renderer(VelloMenuRenderer::new())
                .build(),
            tray_icon: None,
            window_menu: None,
            tray_menu: None,
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

        let mut tray_attributes =
            winit_extras::TrayIconAttributes::default().with_tooltip("Vello Context Menu Example");
        if let Some(icon) = icon.clone() {
            tray_attributes = tray_attributes.with_icon(icon);
        }

        self.tray_icon = match self.tray.create_tray(tray_attributes) {
            Ok(tray) => Some(tray),
            Err(err) => {
                error!(%err, "failed to create tray");
                event_loop.exit();
                return;
            }
        };

        let window = match event_loop.create_window(
            WindowAttributes::default()
                .with_window_icon(icon)
                .with_title("Vello Context Menu - Right-click window or tray icon!"),
        ) {
            Ok(w) => Rc::new(w),
            Err(err) => {
                error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        let size = window.surface_size();
        let ctx = softbuffer::Context::new(window.clone()).unwrap();
        let mut surface = softbuffer::Surface::new(&ctx, window.clone()).unwrap();
        surface
            .resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )
            .unwrap();
        self.surface = Some(surface);

        // Window context menu (vello-rendered)
        let window_items = vec![
            MenuEntry::Item(MenuItem::new(Action::Action1, "Action 1")),
            MenuEntry::Item(MenuItem::new(Action::Action2, "Action 2")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(Action::Settings, "Settings").enabled(false)),
            MenuEntry::Item(MenuItem::new(Action::About, "About")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(Action::Exit, "Exit")),
        ];
        self.window_menu = match self
            .tray
            .create_menu(event_loop, window.as_ref(), window_items)
        {
            Ok(menu) => Some(menu),
            Err(err) => {
                error!(%err, "failed to create window menu");
                None
            }
        };

        // Tray context menu (vello-rendered)
        let tray_items = vec![
            MenuEntry::Item(MenuItem::new(Action::ShowWindow, "Show Window")),
            MenuEntry::Separator,
            MenuEntry::Item(MenuItem::new(Action::Exit, "Exit")),
        ];
        self.tray_menu = match self
            .tray
            .create_menu(event_loop, window.as_ref(), tray_items)
        {
            Ok(menu) => Some(menu),
            Err(err) => {
                error!(%err, "failed to create tray menu");
                None
            }
        };

        window.request_redraw();
        self.window = Some(window);
        info!("Right-click in the window or on the tray icon for vello-rendered menus!");
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.tray.try_recv() {
            match event {
                Event::PointerButton {
                    state: ElementState::Released,
                    button: winit::event::ButtonSource::Mouse(MouseButton::Right),
                    position,
                    ..
                } => {
                    if let Some(menu) = &self.tray_menu {
                        let pos = PhysicalPosition::new(position.x as i32, position.y as i32);
                        menu.show_at_screen_pos(pos);
                    }
                }
                Event::MenuItemClicked { id } => match id {
                    Action::ShowWindow => {
                        if let Some(window) = &self.window {
                            window.focus_window();
                        }
                    }
                    Action::Action1 => info!("Action 1 clicked"),
                    Action::Action2 => info!("Action 2 clicked"),
                    Action::Settings => info!("Settings clicked"),
                    Action::About => info!("About clicked"),
                    Action::Exit => {
                        info!("Exit clicked");
                        event_loop.exit();
                    }
                },
                _ => {}
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        wid: WindowId,
        event: WindowEvent,
    ) {
        if self.tray.handle_window_event(wid, &event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("close requested");
                event_loop.exit();
            }
            WindowEvent::PointerButton {
                state: ElementState::Released,
                button: winit::event::ButtonSource::Mouse(MouseButton::Right),
                position,
                ..
            } => {
                if let Some(menu) = &self.window_menu {
                    let pos = PhysicalPosition::new(position.x as i32, position.y as i32);
                    menu.show(pos);
                }
            }
            WindowEvent::SurfaceResized(size) => {
                if size.width > 0
                    && size.height > 0
                    && let Some(surface) = &mut self.surface
                {
                    let _ = surface.resize(
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );
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
    info!("Starting vello context menu example...");
    event_loop.run_app(app)?;

    Ok(())
}
