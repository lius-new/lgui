#[cfg(any(feature = "renderer-gdi", feature = "renderer-d2d"))]
mod application;
#[cfg(feature = "multi-window")]
mod backbuffer;
#[cfg(feature = "clipboard")]
mod clipboard;
#[cfg(feature = "renderer-d2d")]
mod d2d;
#[cfg(feature = "open-url")]
mod desktop;
mod dispatcher;
mod dpi;
#[cfg(any(feature = "advanced-rendering", feature = "renderer-d2d"))]
pub mod enhanced;
#[cfg(feature = "backend-win32")]
mod fonts;
#[cfg(feature = "renderer-gdi")]
mod gdi;
#[cfg(feature = "multi-window")]
mod hidden_window;
#[cfg(feature = "images")]
mod image_cache;
#[cfg(feature = "notifications")]
mod notifications;
#[cfg(feature = "diagnostics")]
pub mod render_trace;
#[cfg(feature = "svg")]
mod svg;
#[cfg(feature = "system-diagnostics")]
mod system_usage;
#[cfg(feature = "tray")]
mod tray;

#[cfg(any(feature = "renderer-gdi", feature = "renderer-d2d"))]
pub use application::{GdiRendererFactory, Win32Application, Win32Renderer, Win32RendererFactory};
#[cfg(feature = "multi-window")]
pub use backbuffer::{rect_size, AlphaPolicy, LayeredBackbuffer};
#[cfg(feature = "clipboard")]
pub use clipboard::{read_clipboard_text, write_clipboard_text, Win32Clipboard};
#[cfg(feature = "renderer-d2d")]
pub use d2d::{probe_d2d_support, D2dRenderer, D2dRendererFactory};
pub use dispatcher::{Win32DispatchResult, Win32Dispatcher, WM_LGUI_DISPATCH};
pub use dpi::{set_scale_preference, work_area_for_point, work_area_for_window, DpiContext};
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
#[cfg(feature = "images")]
pub use image_cache::{
    cached_image_data, clear_cached_image_cache,
    clear_decoded_image_cache as clear_cached_decoded_image_cache, clear_image_repaint_hwnd,
    draw_cached_image, mark_image_cache_repaint_handled, register_image_repaint_hwnd,
    request_cached_image, set_remote_image_loader, CachedImageStatus, ImageFit as CachedImageFit,
    ImageSource as CachedImageSource, RemoteImageCompletion, RemoteImageLoader,
    WM_IMAGE_CACHE_INVALIDATED,
};
#[cfg(feature = "images")]
pub use image_cache::{ImageFit, ImageSource};
#[cfg(feature = "notifications")]
pub use notifications::Win32NotificationService;
#[cfg(feature = "svg")]
pub use svg::{
    draw_svg_icon, install_svg_font_registry, install_svg_icon_registry, rasterize_svg_icon_bgra,
    SvgBitmap, SvgFontRegistry, SvgIconRegistry, SvgIconSource,
};
#[cfg(feature = "tray")]
pub(crate) use tray::Win32TrayHost;
#[cfg(feature = "tray")]
pub use tray::{taskbar_created_message, TrayIconHandle, Win32TrayIcon, TRAY_MESSAGE_ID};

#[cfg(feature = "open-url")]
pub(crate) use desktop::open_external_url;
#[cfg(feature = "system-diagnostics")]
pub(crate) use system_usage::{sample_interval_ms, snapshot_system_usage};
