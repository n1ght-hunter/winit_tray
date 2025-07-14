use std::fmt;

/// Identifier of a tray window. Unique for each tray window.
///
/// Can be obtained with [`tray.id()`][`crate::Tray::id`].
///
/// Whenever you receive an event specific to a tray window, this event contains a `TrayId` which you
/// can then compare to the ids of your tray windows to determine which one the event is for.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrayId(usize);

impl TrayId {
    /// Convert the `TrayId` into the underlying integer.
    ///
    /// This is useful if you need to pass the ID across an FFI boundary, or store it in an atomic.
    pub const fn into_raw(self) -> usize {
        self.0
    }

    /// Construct a `TrayId` from the underlying integer.
    ///
    /// This should only be called with integers returned from [`TrayId::into_raw`].
    pub const fn from_raw(id: usize) -> Self {
        Self(id)
    }
}

impl fmt::Debug for TrayId {
    fn fmt(&self, fmtr: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmtr)
    }
}
