use std::fmt;

/// Unique identifier for a tray icon.
///
/// Obtained via [`TrayIcon::id`][`crate::TrayIcon::id`]. Every [`Event`][`crate::Event`]
/// that originates from a tray icon click carries the `TrayIconId` of the icon
/// that was clicked, so applications with multiple trays can disambiguate events.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrayIconId(usize);

impl TrayIconId {
    /// Convert the `TrayIconId` into the underlying integer.
    ///
    /// Useful for passing the ID across an FFI boundary or storing it in an atomic.
    pub const fn into_raw(self) -> usize {
        self.0
    }

    /// Construct a `TrayIconId` from a raw integer.
    ///
    /// Should only be called with integers returned from [`TrayIconId::into_raw`].
    pub const fn from_raw(id: usize) -> Self {
        Self(id)
    }
}

impl fmt::Debug for TrayIconId {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmtr)
    }
}
