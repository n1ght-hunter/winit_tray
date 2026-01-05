use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageA;

/// A lazily-initialized window message ID.
pub struct LazyMessageId {
    /// The ID.
    id: AtomicU32,

    /// The name of the message.
    name: &'static str,
}

/// An invalid custom window ID.
const INVALID_ID: u32 = 0x0;

impl LazyMessageId {
    /// Create a new `LazyId`.
    const fn new(name: &'static str) -> Self {
        Self {
            id: AtomicU32::new(INVALID_ID),
            name,
        }
    }

    /// Get the message ID.
    pub fn get(&self) -> u32 {
        // Load the ID.
        let id = self.id.load(Ordering::Relaxed);

        if id != INVALID_ID {
            return id;
        }

        // Register the message.
        // SAFETY: We are sure that the pointer is a valid C string ending with '\0'.
        assert!(self.name.ends_with('\0'));
        let new_id = unsafe { RegisterWindowMessageA(self.name.as_ptr()) };

        assert_ne!(
            new_id,
            0,
            "RegisterWindowMessageA returned zero for '{}': {}",
            self.name,
            std::io::Error::last_os_error()
        );

        // Store the new ID. Since `RegisterWindowMessageA` returns the same value for any given
        // string, the target value will always either be a). `INVALID_ID` or b). the
        // correct ID. Therefore a compare-and-swap operation here (or really any
        // consideration) is never necessary.
        self.id.store(new_id, Ordering::Relaxed);

        new_id
    }
}

// Message sent by a `Window` when it wants to be destroyed by the main thread.
// WPARAM and LPARAM are unused.
pub(crate) static DESTROY_MSG_ID: LazyMessageId = LazyMessageId::new("WinitTray::DestroyMsg\0");

// Message sent by a `Popup` when it wants to be closed.
// WPARAM and LPARAM are unused.
#[cfg(feature = "popup")]
pub(crate) static POPUP_CLOSE_MSG_ID: LazyMessageId =
    LazyMessageId::new("WinitTray::PopupCloseMsg\0");
