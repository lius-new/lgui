#![allow(dead_code)]

pub mod blur;
pub mod d2d;
pub mod gdi_renderer;
pub mod image;
pub mod static_layer;
pub mod static_layer_raster_cache;

pub use gdi_renderer::{
    clear_gdi_renderer_caches, release_gdi_compositing_layer_scope, reset_gdi_frame_blit_metrics,
    take_gdi_frame_blit_metrics, GdiFrameBlitMetrics, GdiFrameBlitSourceMetrics, GdiRenderer,
};

pub(crate) fn portable_render_cache_handle() -> crate::renderer::RenderCacheHandle {
    crate::renderer::RenderCacheHandle::new(
        || {
            static_layer::clear_static_layer_memory_cache();
            clear_gdi_renderer_caches();
            blur::clear_blur_caches();
            crate::assets::clear_image_caches();
        },
        static_layer::clear_scroll_raster_memory_cache,
        static_layer::static_layer_memory_cache_stats,
        static_layer::static_layer_memory_cache_stats_for_prefix,
        static_layer::static_layer_memory_cache_entry_ids_for_prefix,
    )
}
