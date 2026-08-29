use std::{cell::RefCell, sync::Arc};

use crate::core::{PhysicalRect, Scene, ScenePrimitive, UiRect, UiScale};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StaticLayerMemoryCacheStats {
    pub entry_count: usize,
    pub bytes: usize,
    pub budget_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub stores: u64,
    pub evictions: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StaticLayerMemoryCachePrefixStats {
    pub entry_count: usize,
    pub bytes: usize,
}

#[derive(Clone)]
pub struct RenderCacheHandle {
    clear_all: Arc<dyn Fn() + Send + Sync>,
    clear_scroll_raster: Arc<dyn Fn() + Send + Sync>,
    stats: Arc<dyn Fn() -> StaticLayerMemoryCacheStats + Send + Sync>,
    prefix_stats: Arc<dyn Fn(&str) -> StaticLayerMemoryCachePrefixStats + Send + Sync>,
    prefix_entry_ids: Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>,
}

impl RenderCacheHandle {
    pub fn new(
        clear_all: impl Fn() + Send + Sync + 'static,
        clear_scroll_raster: impl Fn() + Send + Sync + 'static,
        stats: impl Fn() -> StaticLayerMemoryCacheStats + Send + Sync + 'static,
        prefix_stats: impl Fn(&str) -> StaticLayerMemoryCachePrefixStats + Send + Sync + 'static,
        prefix_entry_ids: impl Fn(&str) -> Vec<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            clear_all: Arc::new(clear_all),
            clear_scroll_raster: Arc::new(clear_scroll_raster),
            stats: Arc::new(stats),
            prefix_stats: Arc::new(prefix_stats),
            prefix_entry_ids: Arc::new(prefix_entry_ids),
        }
    }
}

thread_local! {
    static RENDER_CACHE: RefCell<Option<RenderCacheHandle>> = const { RefCell::new(None) };
}

pub(crate) struct RenderCacheGuard {
    previous: Option<RenderCacheHandle>,
}

impl Drop for RenderCacheGuard {
    fn drop(&mut self) {
        RENDER_CACHE.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

pub(crate) fn install_render_cache(handle: RenderCacheHandle) -> RenderCacheGuard {
    let previous = RENDER_CACHE.with(|current| current.borrow_mut().replace(handle));
    RenderCacheGuard { previous }
}

pub fn clear_render_caches() {
    RENDER_CACHE.with(|current| {
        if let Some(cache) = current.borrow().as_ref() {
            (cache.clear_all)();
        }
    });
}

pub fn clear_scroll_raster_cache() {
    RENDER_CACHE.with(|current| {
        if let Some(cache) = current.borrow().as_ref() {
            (cache.clear_scroll_raster)();
        }
    });
}

pub fn static_layer_cache_stats() -> StaticLayerMemoryCacheStats {
    RENDER_CACHE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or_else(StaticLayerMemoryCacheStats::default, |cache| {
                (cache.stats)()
            })
    })
}

pub fn static_layer_cache_stats_for_prefix(prefix: &str) -> StaticLayerMemoryCachePrefixStats {
    RENDER_CACHE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or_else(StaticLayerMemoryCachePrefixStats::default, |cache| {
                (cache.prefix_stats)(prefix)
            })
    })
}

pub fn static_layer_cache_entry_ids_for_prefix(prefix: &str) -> Vec<String> {
    RENDER_CACHE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |cache| (cache.prefix_entry_ids)(prefix))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderErrorStage {
    Create,
    Prepare,
    Draw,
    Copy,
    Present,
    Commit,
}

impl RenderErrorStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Prepare => "prepare",
            Self::Draw => "draw",
            Self::Copy => "copy",
            Self::Present => "present",
            Self::Commit => "commit",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameReason {
    #[default]
    SceneChange,
    PlatformExposure,
    Resize,
    Recovery,
    Explicit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererCapabilities {
    pub partial_redraw: bool,
    pub retained_surface: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameInfo<'a> {
    viewport: PhysicalRect,
    damage: &'a [PhysicalRect],
    scale: UiScale,
    reason: FrameReason,
    full_redraw: bool,
}

impl<'a> FrameInfo<'a> {
    pub fn new(
        viewport: PhysicalRect,
        damage: &'a [PhysicalRect],
        scale: UiScale,
        reason: FrameReason,
        full_redraw: bool,
    ) -> Self {
        Self {
            viewport,
            damage,
            scale,
            reason,
            full_redraw,
        }
    }

    pub fn viewport(&self) -> PhysicalRect {
        self.viewport
    }

    pub fn damage(&self) -> &'a [PhysicalRect] {
        self.damage
    }

    pub fn scale(&self) -> UiScale {
        self.scale
    }

    pub fn reason(&self) -> FrameReason {
        self.reason
    }

    pub fn is_full_redraw(&self) -> bool {
        self.full_redraw
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub painted_rects: usize,
    pub painted_pixels: u64,
}

impl RenderStats {
    pub fn for_frame(frame: &FrameInfo<'_>) -> Self {
        let rects = if frame.is_full_redraw() {
            std::slice::from_ref(&frame.viewport)
        } else {
            frame.damage
        };
        Self {
            painted_rects: rects.len(),
            painted_pixels: rects.iter().fold(0_u64, |total, rect| {
                total.saturating_add(
                    (rect.width().max(0) as u64).saturating_mul(rect.height().max(0) as u64),
                )
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    Moderate,
    Critical,
}

pub trait SceneRenderer: 'static {
    type Target;
    type Error;

    fn capabilities(&self) -> RendererCapabilities;

    fn prepare(
        &mut self,
        _target: &mut Self::Target,
        _frame: &FrameInfo<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn render(
        &mut self,
        target: &mut Self::Target,
        scene: &Scene,
        frame: &FrameInfo<'_>,
    ) -> Result<RenderStats, Self::Error>;

    fn trim(&mut self, _pressure: MemoryPressure) {}

    fn reset(&mut self) {}
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

#[cfg(test)]
mod tests {
    use super::*;

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
                || {},
                || {},
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
}
