//! Windows desktop-service adapters.

#[cfg(feature = "notifications-win32")]
mod notification;
#[cfg(feature = "system-diagnostics")]
mod system_usage;
#[cfg(feature = "tray-win32")]
mod tray;

#[cfg(all(
    feature = "notifications-win32",
    any(feature = "backend-win32", feature = "backend-winit")
))]
pub(crate) use notification::install_notification_service;
#[cfg(feature = "notifications-win32")]
pub use notification::Win32NotificationService;
#[cfg(feature = "system-diagnostics")]
pub(crate) use system_usage::{sample_interval_ms, snapshot_system_usage};
#[cfg(all(
    feature = "tray-win32",
    any(feature = "backend-win32", feature = "backend-winit")
))]
pub(crate) use tray::Win32TrayHost;
#[cfg(feature = "tray-win32")]
pub use tray::{taskbar_created_message, TrayIconHandle, Win32TrayIcon, TRAY_MESSAGE_ID};
