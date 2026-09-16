pub mod dpi;

mod runtime;

pub use crate::services::{
    Clipboard, ClipboardError, ClipboardHandle, Notification, NotificationError,
    NotificationHandle, NotificationService, TrayMenuEntry, TrayMenuItem, TrayService,
};
pub use runtime::{task_spawner, InputSink, WakeHandle};

#[cfg(feature = "renderer-skia")]
pub(crate) use crate::renderer::skia;

#[cfg(all(
    target_os = "windows",
    any(
        feature = "backend-win32",
        feature = "notifications-win32",
        feature = "tray-win32"
    )
))]
#[allow(unsafe_code)]
pub mod win32;

#[cfg(feature = "backend-winit")]
mod winit;
#[cfg(feature = "accessibility")]
#[path = "winit/accessibility.rs"]
mod winit_accessibility;
#[cfg(feature = "renderer-skia-gl")]
#[allow(unsafe_code)]
#[path = "winit/surface/gl.rs"]
mod winit_skia_gl;
#[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
#[allow(unsafe_code)]
#[path = "winit/surface/metal.rs"]
mod winit_skia_metal;
#[cfg(all(
    feature = "renderer-skia-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
#[allow(unsafe_code)]
#[path = "winit/surface/vulkan.rs"]
mod winit_skia_vulkan;
#[cfg(all(feature = "backend-winit", target_os = "windows"))]
#[path = "winit/windows.rs"]
mod winit_windows;

#[cfg(feature = "backend-winit")]
pub use winit::{WinitApplication, WinitApplicationError};
