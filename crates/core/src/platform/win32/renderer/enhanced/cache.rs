pub(crate) fn portable_render_cache_handle() -> crate::renderer::RenderCacheHandle {
    crate::renderer::RenderCacheHandle::new(
        super::static_layer::static_layer_memory_cache_stats,
        super::static_layer::static_layer_memory_cache_stats_for_prefix,
        super::static_layer::static_layer_memory_cache_entry_ids_for_prefix,
    )
}
