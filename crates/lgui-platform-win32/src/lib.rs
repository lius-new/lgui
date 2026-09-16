#![cfg(target_os = "windows")]

#[cfg(feature = "backend-win32")]
#[path = "application/mod.rs"]
mod application;
#[cfg(feature = "backend-win32")]
#[path = "window/background.rs"]
mod background;
#[cfg(feature = "backend-win32")]
#[path = "application/dispatcher.rs"]
mod dispatcher;
#[cfg(feature = "backend-win32")]
#[path = "window/dpi.rs"]
mod dpi;
#[cfg(feature = "backend-win32")]
#[path = "assets/fonts.rs"]
mod fonts;
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
#[cfg(feature = "images-win32")]
#[doc(hidden)]
pub mod render_support;
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

#[cfg(feature = "backend-win32")]
pub use application::{
    Win32Application, Win32RenderError, Win32RenderTarget, Win32RendererFactory,
    Win32SceneRenderer, Win32WindowOptions,
};
#[cfg(feature = "backend-win32")]
#[doc(hidden)]
pub use dispatcher::CoalescedTrim;
#[cfg(feature = "backend-win32")]
pub use dispatcher::{Win32DispatchResult, Win32Dispatcher, WM_LGUI_DISPATCH};
#[cfg(feature = "backend-win32")]
pub use dpi::{set_scale_preference, work_area_for_point, work_area_for_window, DpiContext};
#[cfg(feature = "backend-win32")]
#[doc(hidden)]
pub use fonts::portable_text_system_handle;
#[cfg(feature = "backend-win32")]
pub use fonts::{
    apply_dwrite_font_fallback, install_private_font, measure_gdi_text_width_with_fallback,
    measure_text_width, release_private_fonts, set_ui_font_families, set_ui_font_family,
    ui_font_family, ui_font_family_at, ui_font_family_count, ui_font_family_names,
};
#[cfg(feature = "images-win32")]
#[doc(hidden)]
pub use gdiplus::GdiPlusRuntime;
#[cfg(feature = "multi-window")]
pub use hidden_window::Win32HiddenWindow;
#[cfg(feature = "images-win32")]
pub use image_cache::{
    cached_image_data, clear_image_repaint_hwnd, draw_cached_image, register_image_repaint_hwnd,
    request_cached_image, CachedImageStatus, ImageFit as CachedImageFit,
    ImageSource as CachedImageSource, WM_IMAGE_CACHE_INVALIDATED,
};
#[cfg(feature = "images-win32")]
pub(crate) use image_cache::{
    decoded_image_cache_usage, install_image_memory_governor, install_remote_image_loader,
    portable_image_cache_handle, set_decoded_image_cache_budget, take_image_cache_invalidations,
    trim_decoded_image_cache,
};
#[cfg(feature = "images-win32")]
pub use image_cache::{ImageFit, ImageSource};
#[cfg(feature = "notifications-win32")]
pub use services::{
    install_notification_service, Win32NotificationApplicationExt, Win32NotificationService,
};
#[cfg(feature = "tray-win32")]
pub use services::{
    taskbar_created_message, TrayIconHandle, Win32TrayHost, Win32TrayIcon, TRAY_MESSAGE_ID,
};
#[cfg(all(
    feature = "svg",
    any(feature = "backend-win32", feature = "tray-win32")
))]
pub use svg::{
    draw_svg_icon, install_svg_font_registry, install_svg_icon_registry, rasterize_svg_icon_bgra,
    SvgBitmap, SvgFontRegistry,
};

#[cfg(feature = "system-diagnostics")]
pub use services::{system_usage_sample_interval_ms, system_usage_snapshot};
