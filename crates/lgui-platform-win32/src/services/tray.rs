//! Native Windows notification-icon adapter.

#[cfg(feature = "backend-winit")]
mod host;
mod icon;
#[cfg(feature = "backend-winit")]
mod menu;
mod support;

#[cfg(feature = "backend-winit")]
#[doc(hidden)]
pub use host::Win32TrayHost;
pub use icon::{TrayIconHandle, Win32TrayIcon};
pub use support::taskbar_created_message;

pub const TRAY_MESSAGE_ID: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[cfg(all(test, feature = "backend-winit"))]
#[path = "tray/tray_test.rs"]
mod tests;
