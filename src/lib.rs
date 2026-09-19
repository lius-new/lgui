#![deny(unsafe_code)]

#[cfg(feature = "renderer-skia")]
mod renderer_selection;

pub use lgui_core::*;

#[cfg(feature = "images")]
pub mod assets {
    pub use lgui_assets::*;
}
#[cfg(feature = "images")]
pub use lgui_assets::{
    AssetBytes, AssetError, AssetResolver, CustomPaintProvider, ImageCacheHandle, ImageData,
    ImageLoader, ImageSource, ImageStatus, RemoteImageLoader, RemoteImageLoaderHandle,
    RenderResources,
};
#[cfg(feature = "svg")]
pub use lgui_assets::{AssetsApplicationExt, SvgRenderer};

#[cfg(feature = "svg")]
pub mod icons {
    pub use lgui_assets::icons::*;
}

#[cfg(feature = "diagnostics")]
pub mod diagnostics {
    pub use lgui_diagnostics::*;
}
#[cfg(feature = "diagnostics")]
pub use lgui_diagnostics::{
    DiagnosticPresentMode, DiagnosticsApplicationExt, DiagnosticsProvider, DiagnosticsSink,
    FrameCollector, FrameDiagnosticsSnapshot, FramePresentMetrics, FrameRenderMetrics, FrameSample,
};

pub mod services {
    pub use lgui_services::*;
}
pub use lgui_services::{
    Clipboard, ClipboardError, ClipboardHandle, Notification, NotificationError,
    NotificationHandle, NotificationService, ServicesApplicationExt, ServicesContextExt,
    TrayMenuEntry, TrayMenuItem, TrayService,
};
#[cfg(feature = "tray")]
pub use lgui_services::{TrayAction, TrayOptions};

#[cfg(feature = "clipboard")]
pub use lgui_services::clipboard;
#[cfg(feature = "dialogs")]
pub use lgui_services::dialogs;
#[cfg(feature = "dialogs")]
pub use lgui_services::dialogs::{
    FileDialogFilter, FileDialogHandle, FileDialogOptions, FileDialogService,
};
#[cfg(feature = "open-url")]
pub use lgui_services::open_url as desktop;
#[cfg(feature = "open-url")]
pub use lgui_services::open_url::{OpenUrlError, OpenUrlHandle, UrlOpener};

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
}

pub mod prelude {
    #[cfg(feature = "renderer-skia")]
    pub use crate::{RendererKind, RendererProbeError};
    #[cfg(feature = "images")]
    pub use lgui_assets::{
        AssetBytes, AssetError, AssetResolver, CustomPaintProvider, ImageCacheHandle, ImageData,
        ImageLoader, ImageSource, ImageStatus, RemoteImageLoader, RemoteImageLoaderHandle,
        RenderResources,
    };
    #[cfg(feature = "svg")]
    pub use lgui_assets::{AssetsApplicationExt, SvgRenderer};
    pub use lgui_core::prelude::*;
    #[cfg(feature = "diagnostics")]
    pub use lgui_diagnostics::{
        DiagnosticPresentMode, DiagnosticsApplicationExt, DiagnosticsProvider, DiagnosticsSink,
        FrameCollector, FrameDiagnosticsSnapshot, FramePresentMetrics, FrameRenderMetrics,
        FrameSample,
    };
    #[cfg(all(target_os = "windows", feature = "notifications-win32"))]
    pub use lgui_platform_win32::Win32NotificationApplicationExt;
    pub use lgui_render_api::{
        ClipRegion, FrameInfo, FrameReason, GraphicsPreference, MemoryPressure, RenderStats,
        RendererCapabilities, SceneRenderer,
    };
    #[cfg(feature = "router")]
    pub use lgui_router::*;
    #[cfg(feature = "dialogs")]
    pub use lgui_services::dialogs::{
        FileDialogFilter, FileDialogHandle, FileDialogOptions, FileDialogService,
    };
    #[cfg(feature = "open-url")]
    pub use lgui_services::open_url::{OpenUrlError, OpenUrlHandle, UrlOpener};
    pub use lgui_services::{
        Clipboard, ClipboardError, ClipboardHandle, Notification, NotificationError,
        NotificationHandle, NotificationService, ServicesApplicationExt, ServicesContextExt,
        TrayMenuEntry, TrayMenuItem, TrayService,
    };
    #[cfg(feature = "tray")]
    pub use lgui_services::{TrayAction, TrayOptions};
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
    ClipRegion, FrameInfo, FrameReason, GraphicsPreference, MemoryPressure, RenderStats,
    RendererCapabilities, SceneRenderer,
};
#[cfg(feature = "renderer-skia")]
pub use renderer_selection::{RendererKind, RendererProbeError};

#[cfg(all(target_os = "windows", feature = "system-diagnostics"))]
pub fn system_usage_snapshot() -> lgui_diagnostics::SystemUsageSnapshot {
    lgui_platform_win32::system_usage_snapshot()
}

#[cfg(all(target_os = "windows", feature = "system-diagnostics"))]
pub fn system_usage_sample_interval_ms() -> u64 {
    lgui_platform_win32::system_usage_sample_interval_ms()
}

#[cfg(all(target_os = "windows", feature = "tray-win32"))]
pub use lgui_platform_win32::{TrayIconHandle, Win32TrayIcon};
#[cfg(all(target_os = "windows", feature = "notifications-win32"))]
pub use lgui_platform_win32::{Win32NotificationApplicationExt, Win32NotificationService};
#[cfg(feature = "backend-winit")]
pub use lgui_platform_winit::{WinitApplication, WinitApplicationError};
#[cfg(feature = "renderer-skia")]
pub use lgui_render_skia as render_skia;
