pub mod blur;
#[cfg(feature = "advanced-rendering")]
mod cache;
pub mod d2d;
#[cfg(feature = "gdi")]
pub mod gdi_renderer;
pub mod image;
pub mod static_layer;

#[cfg(feature = "advanced-rendering")]
pub(crate) use cache::portable_render_cache_handle;
#[cfg(feature = "gdi")]
pub(crate) use gdi_renderer::{
    gdi_renderer_cache_usage, set_gdi_renderer_cache_budget, trim_gdi_renderer_caches,
};
#[cfg(feature = "gdi")]
pub use gdi_renderer::{
    release_gdi_compositing_layer_scope, reset_gdi_frame_blit_metrics, take_gdi_frame_blit_metrics,
    GdiFrameBlitMetrics, GdiFrameBlitSourceMetrics, GdiRenderer,
};
