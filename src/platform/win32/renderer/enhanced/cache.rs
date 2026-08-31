pub(crate) fn portable_render_cache_handle() -> crate::renderer::RenderCacheHandle {
    crate::renderer::RenderCacheHandle::new(
        || {
            super::static_layer::clear_static_layer_memory_cache();
            super::clear_gdi_renderer_caches();
            super::blur::clear_blur_caches();
            crate::assets::clear_image_caches();
        },
        super::static_layer::clear_scroll_raster_memory_cache,
        super::static_layer::static_layer_memory_cache_stats,
        super::static_layer::static_layer_memory_cache_stats_for_prefix,
        super::static_layer::static_layer_memory_cache_entry_ids_for_prefix,
    )
}
