#![deny(unsafe_code)]

#[cfg(all(target_os = "windows", feature = "backend-win32"))]
mod win32;

pub use lgui_core::*;

#[cfg(feature = "router")]
pub mod router {
    pub use lgui_router::*;
}
#[cfg(feature = "router")]
pub use lgui_router::*;

#[cfg(feature = "store")]
pub mod store {
    pub use lgui_store::*;
}
#[cfg(feature = "store")]
pub use lgui_store::*;

#[cfg(feature = "theme")]
pub mod theme {
    pub use lgui_widgets::theme::*;
}
#[cfg(feature = "theme")]
pub use lgui_widgets::{ColorTokens, SpacingTokens, ThemeContext, ThemeTokens, TypographyTokens};

#[cfg(feature = "widgets")]
pub mod widgets {
    pub use lgui_widgets::widgets::*;
}
#[cfg(feature = "widgets")]
pub use lgui_widgets::widgets::*;

pub mod renderer {
    pub use lgui_core::renderer::*;
    pub use lgui_render_api::*;
    #[cfg(all(
        target_os = "windows",
        any(feature = "renderer-gdi", feature = "renderer-d2d")
    ))]
    pub use lgui_render_win32 as win32;
}

pub mod prelude {
    #[cfg(all(target_os = "windows", feature = "backend-win32"))]
    pub use crate::Win32Application;
    pub use lgui_core::prelude::*;
    #[cfg(all(target_os = "windows", feature = "notifications-win32"))]
    pub use lgui_platform_win32::Win32NotificationApplicationExt;
    pub use lgui_render_api::{
        ClipRegion, FrameInfo, FrameReason, MemoryPressure, RenderStats, RendererCapabilities,
        SceneRenderer,
    };
    #[cfg(feature = "router")]
    pub use lgui_router::*;
    #[cfg(feature = "store")]
    pub use lgui_store::*;
    #[cfg(feature = "widgets")]
    pub use lgui_widgets::widgets::*;
    #[cfg(feature = "theme")]
    pub use lgui_widgets::{
        ColorTokens, SpacingTokens, ThemeContext, ThemeTokens, TypographyTokens,
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
#[cfg(all(target_os = "windows", feature = "tray-win32"))]
pub use lgui_platform_win32::{TrayIconHandle, Win32TrayIcon};
#[cfg(all(target_os = "windows", feature = "notifications-win32"))]
pub use lgui_platform_win32::{Win32NotificationApplicationExt, Win32NotificationService};
#[cfg(all(target_os = "windows", feature = "backend-win32"))]
pub use lgui_platform_win32::{
    Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
    Win32WindowOptions,
};
#[cfg(feature = "backend-winit")]
pub use lgui_platform_winit::{WinitApplication, WinitApplicationError};
#[cfg(feature = "renderer-skia")]
pub use lgui_render_skia as render_skia;
#[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
pub use lgui_render_win32::{D2dRenderer, D2dRendererFactory};
#[cfg(all(target_os = "windows", feature = "renderer-gdi"))]
pub use lgui_render_win32::{GdiRenderer, GdiRendererFactory};
#[cfg(all(target_os = "windows", feature = "backend-win32"))]
pub use win32::Win32Application;
