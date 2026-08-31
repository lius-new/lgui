pub mod blur;
#[cfg(feature = "advanced-rendering")]
mod cache;
pub mod d2d;
#[cfg(feature = "renderer-gdi")]
pub mod gdi_renderer;
pub mod image;
pub mod static_layer;
pub mod static_layer_raster_cache;

#[cfg(feature = "advanced-rendering")]
pub(crate) use cache::portable_render_cache_handle;
#[cfg(feature = "renderer-gdi")]
pub use gdi_renderer::{
    clear_gdi_renderer_caches, release_gdi_compositing_layer_scope, reset_gdi_frame_blit_metrics,
    take_gdi_frame_blit_metrics, GdiFrameBlitMetrics, GdiFrameBlitSourceMetrics, GdiRenderer,
};
