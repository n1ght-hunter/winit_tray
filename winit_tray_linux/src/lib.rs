#![cfg(target_os = "linux")]

mod dbus_interface;
mod util;

#[cfg(feature = "menu")]
pub mod menu;

use std::marker::PhantomData;
use std::sync::Arc;
use std::thread;

use anyhow::{anyhow, Context, Result};
use tracing::{debug, error, trace, warn};
use winit_tray_core::{Tray as CoreTray, TrayAttributes, TrayProxy};
use zbus::blocking::Connection;

use dbus_interface::StatusNotifierItemInterface;
use util::{icon_to_sni_icon, SniIcon};

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

const SNI_OBJECT_PATH: &str = "/StatusNotifierItem";
const SNI_WATCHER_SERVICE: &str = "org.kde.StatusNotifierWatcher";
const SNI_WATCHER_PATH: &str = "/StatusNotifierWatcher";

/// Linux system tray icon implementation using StatusNotifierItem.
pub struct Tray<T = ()> {
    internal_id: usize,
    // Handle to the background thread that processes D-Bus messages
    thread_handle: Option<thread::JoinHandle<()>>,
    // Channel to signal the background thread to stop
    shutdown_tx: Option<std::sync::mpsc::Sender<()>>,
    _marker: PhantomData<T>,
}

impl<T> std::fmt::Debug for Tray<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("internal_id", &self.internal_id)
            .finish()
    }
}

impl<T: Clone + Send + Sync + 'static> Tray<T> {
    pub fn new(proxy: TrayProxy<T>, attr: TrayAttributes<T>) -> Result<Self> {
        let internal_id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tray_id = winit_tray_core::tray_id::TrayId::from_raw(internal_id);

        debug!(internal_id, "Creating new Linux tray icon");

        // Convert icon to SNI format
        let icon_pixmap = if let Some(icon) = &attr.icon {
            icon_to_sni_icon(icon)
                .map(|i| vec![i])
                .unwrap_or_else(Vec::new)
        } else {
            Vec::new()
        };

        // Generate unique ID for this tray
        let id = format!("winit_tray_{}", internal_id);
        let title = attr.tooltip.unwrap_or_else(|| "Tray Icon".to_string());

        // Wrap proxy in Arc for sharing across threads
        let proxy = Arc::new(proxy);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

        // Spawn background thread for D-Bus message processing
        let thread_handle = thread::spawn(move || {
            if let Err(e) = run_dbus_service(id, title, icon_pixmap, tray_id, proxy, shutdown_rx) {
                error!("D-Bus service error: {}", e);
            }
        });

        Ok(Tray {
            internal_id,
            thread_handle: Some(thread_handle),
            shutdown_tx: Some(shutdown_tx),
            _marker: PhantomData,
        })
    }
}

impl<T> CoreTray for Tray<T> {
    fn id(&self) -> winit_tray_core::tray_id::TrayId {
        winit_tray_core::tray_id::TrayId::from_raw(self.internal_id)
    }
}

impl<T> Drop for Tray<T> {
    fn drop(&mut self) {
        debug!(internal_id = self.internal_id, "Dropping Linux tray icon");

        // Signal the background thread to shutdown
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Wait for the background thread to finish (with timeout)
        if let Some(handle) = self.thread_handle.take() {
            // Give it a short time to clean up
            let timeout = std::time::Duration::from_millis(500);
            let start = std::time::Instant::now();

            while !handle.is_finished() && start.elapsed() < timeout {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            if handle.is_finished() {
                let _ = handle.join();
                debug!("Background thread cleaned up successfully");
            } else {
                warn!("Background thread did not exit cleanly within timeout");
            }
        }
    }
}

/// Runs the D-Bus service on a background thread.
///
/// This function:
/// 1. Connects to the session bus
/// 2. Registers the StatusNotifierItem interface
/// 3. Registers with the StatusNotifierWatcher
/// 4. Processes D-Bus messages in a loop until shutdown signal received
fn run_dbus_service<T: Clone + Send + Sync + 'static>(
    id: String,
    title: String,
    icon_pixmap: Vec<SniIcon>,
    tray_id: winit_tray_core::tray_id::TrayId,
    proxy: Arc<TrayProxy<T>>,
    shutdown_rx: std::sync::mpsc::Receiver<()>,
) -> Result<()> {
    trace!("Starting D-Bus service thread");

    // Connect to session bus
    let connection = Connection::session()
        .context("Failed to connect to D-Bus session bus")?;

    debug!("Connected to D-Bus session bus");

    // Create the StatusNotifierItem interface
    let interface = StatusNotifierItemInterface {
        id: id.clone(),
        title,
        icon_pixmap,
        tray_id,
        proxy,
        menu: {
            #[cfg(feature = "menu")]
            {
                Some(zbus::zvariant::ObjectPath::try_from("/MenuBar")
                    .expect("Invalid menu path"))
            }
            #[cfg(not(feature = "menu"))]
            {
                None
            }
        },
    };

    // Register the interface at the object path
    connection
        .object_server()
        .at(SNI_OBJECT_PATH, interface)
        .context("Failed to register StatusNotifierItem interface")?;

    debug!(path = SNI_OBJECT_PATH, "Registered StatusNotifierItem interface");

    // Register with StatusNotifierWatcher
    if let Err(e) = register_with_watcher(&connection, &id) {
        warn!("Failed to register with StatusNotifierWatcher: {}. Tray icon may not appear.", e);
        // Continue anyway - some DEs might work without explicit registration
    }

    // Keep the D-Bus connection alive and process messages until shutdown
    // Note: zbus automatically processes incoming messages in a background thread,
    // we just need to keep this thread alive and the connection in scope.
    debug!("D-Bus service thread running, waiting for shutdown signal");

    match shutdown_rx.recv() {
        Ok(_) => {
            debug!("Received shutdown signal, cleaning up");
        }
        Err(_) => {
            debug!("Shutdown channel disconnected, exiting");
        }
    }

    // Unregister from StatusNotifierWatcher before exiting
    if let Err(e) = unregister_from_watcher(&connection, &id) {
        warn!("Failed to unregister from StatusNotifierWatcher: {}", e);
    }

    // Remove the interface from the object server
    let _ = connection.object_server().remove::<StatusNotifierItemInterface<T>, _>(SNI_OBJECT_PATH);

    debug!("D-Bus service thread exiting cleanly");
    Ok(())
}

/// Registers this tray icon with the StatusNotifierWatcher.
///
/// The StatusNotifierWatcher is a system service that keeps track of all
/// active tray icons and notifies the desktop environment about them.
fn register_with_watcher(connection: &Connection, _id: &str) -> Result<()> {
    trace!("Registering with StatusNotifierWatcher");

    // Get the unique name of our connection
    let unique_name = connection.unique_name()
        .ok_or_else(|| anyhow!("Failed to get D-Bus unique name"))?;

    // Create service name: unique_name + object_path
    let service_name = format!("{}{}", unique_name, SNI_OBJECT_PATH);

    debug!(service = %service_name, "Calling RegisterStatusNotifierItem");

    // Call RegisterStatusNotifierItem on the watcher
    let proxy = zbus::blocking::Proxy::new(
        connection,
        SNI_WATCHER_SERVICE,
        SNI_WATCHER_PATH,
        "org.kde.StatusNotifierWatcher",
    )?;

    proxy.call::<&str, _, ()>("RegisterStatusNotifierItem", &service_name)
        .context("Failed to call RegisterStatusNotifierItem")?;

    debug!("Successfully registered with StatusNotifierWatcher");
    Ok(())
}

/// Unregisters this tray icon from the StatusNotifierWatcher.
///
/// This should be called before the tray is destroyed to ensure the icon
/// disappears from the system tray immediately.
fn unregister_from_watcher(connection: &Connection, _id: &str) -> Result<()> {
    trace!("Unregistering from StatusNotifierWatcher");

    // Get the unique name of our connection
    let unique_name = connection.unique_name()
        .ok_or_else(|| anyhow!("Failed to get D-Bus unique name"))?;

    // Create service name: unique_name + object_path
    let service_name = format!("{}{}", unique_name, SNI_OBJECT_PATH);

    debug!(service = %service_name, "Calling UnregisterStatusNotifierItem");

    // Call UnregisterStatusNotifierItem on the watcher (if it exists)
    match zbus::blocking::Proxy::new(
        connection,
        SNI_WATCHER_SERVICE,
        SNI_WATCHER_PATH,
        "org.kde.StatusNotifierWatcher",
    ) {
        Ok(proxy) => {
            // Some implementations don't have UnregisterStatusNotifierItem,
            // so we ignore errors here
            let _ = proxy.call::<&str, _, ()>("UnregisterStatusNotifierItem", &service_name);
            debug!("Unregistered from StatusNotifierWatcher");
        }
        Err(e) => {
            debug!("Could not create proxy for unregistration: {}", e);
        }
    }

    Ok(())
}
