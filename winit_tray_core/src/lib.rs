use winit::icon::Icon;

pub mod tray_id;

pub enum TrayEvent {
    /// The tray icon was clicked.
    Click,
    /// The tray icon was right-clicked.
    RightClick,
    /// The tray icon was double-clicked.
    DoubleClick,
}

pub type TrayProxy = std::sync::Arc<dyn Fn(tray_id::TrayId, TrayEvent) + Send + Sync>;

pub trait Tray: Send + Sync + std::fmt::Debug {
    fn id(&self) -> tray_id::TrayId;
}


/// Run on main thread
struct Runner {}

#[derive(Debug)]
pub struct TrayAttributes {
    pub tooltip: String,
    pub class_name: String,
    pub icon: Option<Icon>,
    pub(crate) parent_window: Option<SendSyncRawWindowHandle>,
}

impl Default for TrayAttributes {
    fn default() -> Self {
        TrayAttributes {
            tooltip: "Winit Tray".to_string(),
            icon: None,
            parent_window: None,
            class_name: "Window Class".to_string(),
        }
    }
}

impl TrayAttributes {
    /// Set the tooltip for the tray icon.
    pub fn with_tooltip(mut self, title: impl Into<String>) -> Self {
        self.tooltip = title.into();
        self
    }

    /// Set the icon for the tray.
    pub fn with_icon(mut self, icon: Option<Icon>) -> Self {
        self.icon = icon;
        self
    }

    /// Set the class name for the tray window.
    pub fn with_class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = class_name.into();
        self
    }

    /// Build window with parent window.
    ///
    /// The default is `None`.
    ///
    /// ## Safety
    ///
    /// `parent_window` must be a valid window handle.
    ///
    /// ## Platform-specific
    ///
    /// - **Windows** : A child window has the WS_CHILD style and is confined
    ///   to the client area of its parent window. For more information, see
    ///   <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#child-windows>
    /// - **X11**: A child window is confined to the client area of its parent window.
    /// - **Android / iOS / Wayland / Web:** Unsupported.
    #[inline]
    pub unsafe fn with_parent_window(
        mut self,
        parent_window: Option<rwh_06::RawWindowHandle>,
    ) -> Self {
        self.parent_window = parent_window.map(SendSyncRawWindowHandle);
        self
    }

    /// Get the parent window stored on the attributes.
    pub fn parent_window(&self) -> Option<&rwh_06::RawWindowHandle> {
        self.parent_window.as_ref().map(|handle| &handle.0)
    }
}

/// Wrapper for [`rwh_06::RawWindowHandle`] for [`WindowAttributes::parent_window`].
///
/// # Safety
///
/// The user has to account for that when using [`WindowAttributes::with_parent_window()`],
/// which is `unsafe`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SendSyncRawWindowHandle(pub(crate) rwh_06::RawWindowHandle);

unsafe impl Send for SendSyncRawWindowHandle {}
unsafe impl Sync for SendSyncRawWindowHandle {}
