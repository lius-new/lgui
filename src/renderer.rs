use crate::core::{Scene, ScenePrimitive, UiRect};

#[cfg(feature = "advanced-rendering")]
pub use crate::platform::win32::enhanced::static_layer::{
    StaticLayerMemoryCachePrefixStats, StaticLayerMemoryCacheStats,
};

#[cfg(feature = "advanced-rendering")]
pub fn clear_render_caches() {
    crate::platform::win32::enhanced::static_layer::clear_static_layer_memory_cache();
    crate::platform::win32::enhanced::clear_gdi_renderer_caches();
    crate::platform::win32::enhanced::blur::clear_blur_caches();
    crate::assets::clear_image_caches();
}

#[cfg(feature = "advanced-rendering")]
pub fn clear_scroll_raster_cache() {
    crate::platform::win32::enhanced::static_layer::clear_scroll_raster_memory_cache();
}

#[cfg(feature = "advanced-rendering")]
pub fn static_layer_cache_stats() -> StaticLayerMemoryCacheStats {
    crate::platform::win32::enhanced::static_layer::static_layer_memory_cache_stats()
}

#[cfg(feature = "advanced-rendering")]
pub fn static_layer_cache_stats_for_prefix(prefix: &str) -> StaticLayerMemoryCachePrefixStats {
    crate::platform::win32::enhanced::static_layer::static_layer_memory_cache_stats_for_prefix(
        prefix,
    )
}

#[cfg(feature = "advanced-rendering")]
pub fn static_layer_cache_entry_ids_for_prefix(prefix: &str) -> Vec<String> {
    crate::platform::win32::enhanced::static_layer::static_layer_memory_cache_entry_ids_for_prefix(
        prefix,
    )
}

pub trait RenderBackend<Target> {
    fn draw_scene(&mut self, target: Target, scene: &Scene, clip: Option<UiRect>);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRegion {
    rect: UiRect,
}

impl ClipRegion {
    pub fn new(rect: UiRect) -> Self {
        Self { rect }
    }

    pub fn rect(self) -> UiRect {
        self.rect
    }

    pub fn intersects(self, command: &ScenePrimitive) -> bool {
        command.paint_bounds().intersect(self.rect).is_some()
    }

    pub fn intersection(self, rect: UiRect) -> Option<UiRect> {
        rect.intersect(self.rect)
    }
}
