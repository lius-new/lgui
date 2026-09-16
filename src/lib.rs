#![deny(unsafe_code)]

#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
mod renderer_selection;
#[cfg(all(target_os = "windows", feature = "backend-win32"))]
mod win32;

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
    #[cfg(all(
        target_os = "windows",
        any(feature = "renderer-gdi", feature = "renderer-d2d")
    ))]
    pub mod win32 {
        pub use lgui_platform_win32::{
            Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
        };

        #[cfg(feature = "renderer-d2d")]
        pub mod d2d {
            pub use lgui_render_d2d::*;
        }
        #[cfg(feature = "renderer-d2d")]
        pub use lgui_render_d2d::{probe_d2d_support, D2dRenderer, D2dRendererFactory};
        #[cfg(feature = "renderer-gdi")]
        pub mod gdi {
            pub use lgui_render_gdi::*;
        }
        #[cfg(all(feature = "renderer-gdi", feature = "multi-window"))]
        pub use lgui_render_gdi::{rect_size, AlphaPolicy, LayeredBackbuffer};
        #[cfg(feature = "renderer-gdi")]
        pub use lgui_render_gdi::{GdiRenderer, GdiRendererFactory};
    }
}

pub mod prelude {
    #[cfg(all(target_os = "windows", feature = "backend-win32"))]
    pub use crate::Win32Application;
    #[cfg(any(
        all(feature = "renderer-gdi", target_os = "windows"),
        all(feature = "renderer-d2d", target_os = "windows"),
        feature = "renderer-skia"
    ))]
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
#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
pub use renderer_selection::{RendererKind, RendererProbeError};

#[cfg(all(target_os = "windows", feature = "system-diagnostics"))]
pub fn system_usage_snapshot() -> lgui_diagnostics::SystemUsageSnapshot {
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
#[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
pub use lgui_render_d2d::{D2dRenderer, D2dRendererFactory};
#[cfg(all(target_os = "windows", feature = "renderer-gdi"))]
pub use lgui_render_gdi::{GdiRenderer, GdiRendererFactory};
#[cfg(feature = "renderer-skia")]
pub use lgui_render_skia as render_skia;
#[cfg(all(target_os = "windows", feature = "backend-win32"))]
pub use win32::Win32Application;
