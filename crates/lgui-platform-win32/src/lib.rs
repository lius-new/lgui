#![cfg(target_os = "windows")]

#[cfg(feature = "tray-win32")]
#[path = "assets/ico.rs"]
mod ico;
#[cfg(any(
    feature = "notifications-win32",
    feature = "system-diagnostics",
    feature = "tray-win32"
))]
pub mod services;
#[cfg(all(feature = "svg", feature = "tray-win32"))]
#[path = "assets/svg.rs"]
mod svg;

#[cfg(feature = "notifications-win32")]
pub use services::{
    install_notification_service, Win32NotificationApplicationExt, Win32NotificationService,
};
#[cfg(feature = "tray-win32")]
pub use services::{
    taskbar_created_message, TrayIconHandle, Win32TrayHost, Win32TrayIcon, TRAY_MESSAGE_ID,
};
#[cfg(all(feature = "svg", feature = "tray-win32"))]
pub use svg::{
    install_svg_font_registry, install_svg_icon_registry, rasterize_svg_icon_bgra, SvgBitmap,
    SvgFontRegistry,
};

#[cfg(feature = "system-diagnostics")]
pub use services::{system_usage_sample_interval_ms, system_usage_snapshot};
