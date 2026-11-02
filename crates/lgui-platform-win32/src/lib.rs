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

#[cfg(feature = "tray-win32")]
pub use services::{
    TRAY_MESSAGE_ID, TrayIconHandle, Win32TrayHost, Win32TrayIcon, taskbar_created_message,
};
#[cfg(feature = "notifications-win32")]
pub use services::{
    Win32NotificationApplicationExt, Win32NotificationService, install_notification_service,
};
#[cfg(all(feature = "svg", feature = "tray-win32"))]
pub use svg::{
    SvgBitmap, SvgFontRegistry, install_application_svg_icons, install_svg_font_registry,
    install_svg_icon_registry, rasterize_svg_icon_bgra,
};

#[cfg(feature = "system-diagnostics")]
pub use services::{system_usage_sample_interval_ms, system_usage_snapshot};
