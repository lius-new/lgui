#![deny(unsafe_code)]

pub use lgui_core::*;

pub mod renderer {
    pub use lgui_core::renderer::*;
    pub use lgui_render_api::*;
}

pub mod prelude {
    pub use lgui_core::prelude::*;
    #[cfg(all(target_os = "windows", feature = "backend-win32"))]
    pub use lgui_platform_win32::Win32Application;
    #[cfg(all(target_os = "windows", feature = "notifications-win32"))]
    pub use lgui_platform_win32::Win32NotificationApplicationExt;
    pub use lgui_render_api::{
        ClipRegion, FrameInfo, FrameReason, MemoryPressure, RenderStats, RendererCapabilities,
        SceneRenderer,
    };
}

pub use lgui_render_api::{
    ClipRegion, FrameInfo, FrameReason, MemoryPressure, RenderStats, RendererCapabilities,
    SceneRenderer,
};

#[cfg(all(target_os = "windows", feature = "system-diagnostics"))]
pub fn system_usage_snapshot() -> lgui_core::diagnostics::SystemUsageSnapshot {
    lgui_platform_win32::system_usage_snapshot()
}

#[cfg(all(target_os = "windows", feature = "system-diagnostics"))]
pub fn system_usage_sample_interval_ms() -> u64 {
    lgui_platform_win32::system_usage_sample_interval_ms()
}

#[cfg(all(target_os = "windows", feature = "backend-win32"))]
pub use lgui_platform_win32 as platform_win32;
#[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
pub use lgui_platform_win32::{D2dRenderer, D2dRendererFactory};
#[cfg(all(target_os = "windows", feature = "renderer-gdi"))]
pub use lgui_platform_win32::{GdiRenderer, GdiRendererFactory};
#[cfg(all(target_os = "windows", feature = "tray-win32"))]
pub use lgui_platform_win32::{TrayIconHandle, Win32TrayIcon};
#[cfg(all(
    target_os = "windows",
    feature = "backend-win32",
    any(
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        feature = "renderer-skia"
    )
))]
pub use lgui_platform_win32::{
    Win32Application, Win32RenderError, Win32RenderTarget, Win32RendererFactory,
    Win32SceneRenderer, Win32WindowOptions,
};
#[cfg(all(target_os = "windows", feature = "notifications-win32"))]
pub use lgui_platform_win32::{Win32NotificationApplicationExt, Win32NotificationService};
#[cfg(feature = "backend-winit")]
pub use lgui_platform_winit::{WinitApplication, WinitApplicationError};
#[cfg(feature = "renderer-skia")]
pub use lgui_render_skia as render_skia;
