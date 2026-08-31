#[cfg(all(
    feature = "backend-win32",
    any(
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        feature = "renderer-skia"
    )
))]
#[path = "application/mod.rs"]
mod application;
#[cfg(all(
    feature = "backend-win32",
    any(feature = "multi-window", feature = "renderer-gdi")
))]
#[path = "renderer/backbuffer.rs"]
mod backbuffer;
#[cfg(all(
    feature = "backend-win32",
    any(
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        feature = "renderer-skia"
    )
))]
#[path = "window/background.rs"]
mod background;
#[cfg(feature = "renderer-d2d")]
#[path = "renderer/d2d.rs"]
mod d2d;
#[cfg(feature = "backend-win32")]
#[path = "application/dispatcher.rs"]
mod dispatcher;
#[cfg(feature = "backend-win32")]
#[path = "window/dpi.rs"]
mod dpi;
#[cfg(any(feature = "advanced-rendering", feature = "renderer-d2d"))]
#[path = "renderer/enhanced/mod.rs"]
pub mod enhanced;
#[cfg(feature = "backend-win32")]
#[path = "assets/fonts.rs"]
mod fonts;
#[cfg(feature = "renderer-gdi")]
#[path = "renderer/gdi.rs"]
mod gdi;
#[cfg(feature = "images-win32")]
#[path = "assets/gdiplus.rs"]
mod gdiplus;
#[cfg(feature = "multi-window")]
#[path = "window/hidden_window.rs"]
mod hidden_window;
#[cfg(any(feature = "backend-win32", feature = "tray-win32"))]
#[path = "assets/ico.rs"]
mod ico;
#[cfg(feature = "images-win32")]
#[path = "assets/image_cache.rs"]
mod image_cache;
#[cfg(feature = "diagnostics")]
#[path = "renderer/render_trace.rs"]
pub mod render_trace;
#[cfg(any(
    feature = "notifications-win32",
    feature = "system-diagnostics",
    feature = "tray-win32"
))]
pub mod services;
#[cfg(all(
    feature = "svg",
    any(feature = "backend-win32", feature = "tray-win32")
))]
#[path = "assets/svg.rs"]
mod svg;

#[cfg(feature = "renderer-skia")]
pub use crate::platform::skia::probe_skia_support;
#[cfg(all(
    feature = "backend-win32",
    any(
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        feature = "renderer-skia"
    )
))]
pub use application::{
    GdiRendererFactory, Win32Application, Win32RenderError, Win32RenderTarget,
    Win32RendererFactory, Win32SceneRenderer, Win32WindowOptions,
};
#[cfg(feature = "multi-window")]
pub use backbuffer::{rect_size, AlphaPolicy, LayeredBackbuffer};
#[cfg(feature = "renderer-d2d")]
pub use d2d::{probe_d2d_support, D2dRenderer, D2dRendererFactory};
#[cfg(feature = "backend-win32")]
pub use dispatcher::{Win32DispatchResult, Win32Dispatcher, WM_LGUI_DISPATCH};
#[cfg(feature = "backend-win32")]
pub use dpi::{set_scale_preference, work_area_for_point, work_area_for_window, DpiContext};
#[cfg(feature = "backend-win32")]
pub(crate) use fonts::portable_text_system_handle;
#[cfg(feature = "backend-win32")]
pub use fonts::{
    apply_dwrite_font_fallback, install_private_font, measure_gdi_text_width_with_fallback,
    measure_text_width, release_private_fonts, set_ui_font_families, set_ui_font_family,
    ui_font_family, ui_font_family_at, ui_font_family_count, ui_font_family_names,
};
#[cfg(feature = "renderer-gdi")]
pub use gdi::GdiRenderer;
#[cfg(feature = "multi-window")]
pub use hidden_window::Win32HiddenWindow;
#[cfg(feature = "images-win32")]
pub use image_cache::{
    cached_image_data, clear_cached_image_cache,
    clear_decoded_image_cache as clear_cached_decoded_image_cache, clear_image_repaint_hwnd,
    draw_cached_image, mark_image_cache_repaint_handled, register_image_repaint_hwnd,
    request_cached_image, CachedImageStatus, ImageFit as CachedImageFit,
    ImageSource as CachedImageSource, WM_IMAGE_CACHE_INVALIDATED,
};
#[cfg(feature = "images-win32")]
pub(crate) use image_cache::{install_remote_image_loader, portable_image_cache_handle};
#[cfg(feature = "images-win32")]
pub use image_cache::{ImageFit, ImageSource};
#[cfg(feature = "notifications-win32")]
pub use services::Win32NotificationService;
#[cfg(feature = "tray-win32")]
pub use services::{taskbar_created_message, TrayIconHandle, Win32TrayIcon, TRAY_MESSAGE_ID};
#[cfg(all(
    feature = "svg",
    any(feature = "backend-win32", feature = "tray-win32")
))]
pub use svg::{
    draw_svg_icon, install_svg_font_registry, install_svg_icon_registry, rasterize_svg_icon_bgra,
    SvgBitmap, SvgFontRegistry,
};

#[cfg(feature = "system-diagnostics")]
pub(crate) use services::{sample_interval_ms, snapshot_system_usage};
