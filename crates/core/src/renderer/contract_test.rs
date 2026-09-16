use super::*;
use crate::renderer::{
    install_render_cache, static_layer_cache_entry_ids_for_prefix, static_layer_cache_stats,
    static_layer_cache_stats_for_prefix, RenderCacheHandle, StaticLayerMemoryCachePrefixStats,
    StaticLayerMemoryCacheStats,
};

#[test]
fn frame_stats_use_damage_or_the_full_viewport() {
    let viewport = PhysicalRect::new(0, 0, 100, 80);
    let damage = [
        PhysicalRect::new(0, 0, 10, 20),
        PhysicalRect::new(50, 40, 60, 50),
    ];
    let dirty = FrameInfo::new(
        viewport,
        &damage,
        UiScale::ONE,
        FrameReason::SceneChange,
        false,
    );
    let full = FrameInfo::new(viewport, &damage, UiScale::ONE, FrameReason::Resize, true);

    assert_eq!(
        RenderStats::for_frame(&dirty),
        RenderStats {
            painted_rects: 2,
            painted_pixels: 300,
        }
    );
    assert_eq!(
        RenderStats::for_frame(&full),
        RenderStats {
            painted_rects: 1,
            painted_pixels: 8_000,
        }
    );
}

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
