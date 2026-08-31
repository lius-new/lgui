pub mod dpi;

mod runtime;
#[path = "../services/contracts.rs"]
mod service_contracts;

pub use runtime::{task_spawner, InputSink, WakeHandle};
pub use service_contracts::{
    Clipboard, ClipboardError, ClipboardHandle, Notification, NotificationError,
    NotificationHandle, NotificationService, TrayMenuEntry, TrayMenuItem, TrayService,
};

#[cfg(feature = "renderer-skia")]
pub(crate) use crate::renderer::skia;

#[cfg(all(feature = "backend-win32", target_os = "windows"))]
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
