use super::*;

#[test]
fn render_cache_capability_is_scoped_and_restored() {
    assert_eq!(static_layer_cache_stats(), Default::default());
    {
        let _guard = install_render_cache(RenderCacheHandle::new(
            || StaticLayerMemoryCacheStats {
                entry_count: 3,
                ..Default::default()
            },
            |_| StaticLayerMemoryCachePrefixStats {
                entry_count: 2,
                ..Default::default()
            },
            |prefix| vec![format!("{prefix}.tile")],
        ));
        assert_eq!(static_layer_cache_stats().entry_count, 3);
        assert_eq!(static_layer_cache_stats_for_prefix("page").entry_count, 2);
        assert_eq!(
            static_layer_cache_entry_ids_for_prefix("page"),
            ["page.tile"]
        );
    }
    assert_eq!(static_layer_cache_stats(), Default::default());
}
