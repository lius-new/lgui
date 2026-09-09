//! Native Windows notification-icon adapter.

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
mod host;
mod icon;
#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
mod menu;
mod support;

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
pub(crate) use host::Win32TrayHost;
pub use icon::{TrayIconHandle, Win32TrayIcon};
pub use support::taskbar_created_message;

pub const TRAY_MESSAGE_ID: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[cfg(all(test, any(feature = "backend-win32", feature = "backend-winit")))]
#[path = "tray/tray_test.rs"]
mod tests;
