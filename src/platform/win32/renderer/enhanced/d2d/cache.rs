use super::*;

pub struct D2dRenderer {
    pub(super) context: ID2D1DeviceContext,
    pub(super) dwrite_factory: IDWriteFactory,
    pub(super) scene_bitmap: ID2D1Bitmap1,
    pub(super) bitmap_cache: D2dBitmapCache,
    pub(super) overlay_brush_cache: HashMap<D2dOverlayBrushCacheKey, D2dOverlayBrushSet>,
    pub(super) frame_bitmap_cache: HashMap<D2dBitmapCacheKey, ID2D1Bitmap1>,
    pub(super) compositing_layers: HashMap<UiId, D2dCompositingLayer>,
    pub(super) scene_bytes: usize,
}

pub(super) fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

pub(super) fn raster_size(rect: UiRect) -> (i32, i32) {
    (raster_length(rect.width()), raster_length(rect.height()))
}

pub(super) struct D2dBitmapCacheEntry {
    pub(super) bitmap: ID2D1Bitmap1,
    pub(super) bytes: usize,
    pub(super) last_used: u64,
    pub(super) retention: crate::memory::RetentionClass,
    pub(super) priority: crate::memory::CachePriority,
}

pub(super) struct D2dBitmapCache {
    pub(super) entries: HashMap<D2dBitmapCacheKey, D2dBitmapCacheEntry>,
    pub(super) bytes: usize,
    pub(super) tick: u64,
    pub(super) budget_bytes: usize,
    pub(super) hits: u64,
    pub(super) misses: u64,
    pub(super) evictions: u64,
}

impl D2dBitmapCache {
    pub(super) fn new(budget_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            tick: 0,
            budget_bytes,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub(super) fn retain_live(&mut self, live: &HashSet<D2dBitmapCacheKey>) {
        let mut removed_bytes = 0usize;
        self.entries.retain(|key, entry| {
            let retain = live.contains(key);
            if !retain {
                removed_bytes = removed_bytes.saturating_add(entry.bytes);
            }
            retain
        });
        self.bytes = self.bytes.saturating_sub(removed_bytes);
    }

    pub(super) fn get(&mut self, key: &D2dBitmapCacheKey) -> Option<ID2D1Bitmap1> {
        self.tick = self.tick.saturating_add(1);
        let Some(entry) = self.entries.get_mut(key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        entry.last_used = self.tick;
        Some(entry.bitmap.clone())
    }

    pub(super) fn insert(&mut self, key: D2dBitmapCacheKey, bitmap: ID2D1Bitmap1) {
        self.insert_with_policy(
            key,
            bitmap,
            crate::memory::RetentionClass::Session,
            crate::memory::CachePriority::Normal,
        );
    }

    pub(super) fn insert_with_policy(
        &mut self,
        key: D2dBitmapCacheKey,
        bitmap: ID2D1Bitmap1,
        retention: crate::memory::RetentionClass,
        priority: crate::memory::CachePriority,
    ) {
        self.tick = self.tick.saturating_add(1);
        let bytes = key.estimated_bytes();
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            D2dBitmapCacheEntry {
                bitmap,
                bytes,
                last_used: self.tick,
                retention,
                priority,
            },
        );
    }

    pub(super) fn evict_to_budget(&mut self) {
        let evictions = self.eviction_plan(self.budget_bytes);
        for key in evictions {
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }

    pub(super) fn set_budget(&mut self, budget_bytes: usize) {
        self.budget_bytes = budget_bytes;
        self.evict_to_budget();
    }

    pub(super) fn trim_to(&mut self, target_bytes: usize) -> usize {
        let before = self.bytes;
        let target = target_bytes.min(self.budget_bytes);
        let evictions = self.eviction_plan(target);
        for key in evictions {
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
        before.saturating_sub(self.bytes)
    }

    fn eviction_plan(&self, target: usize) -> Vec<D2dBitmapCacheKey> {
        let mut entries = self.entries.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(_, entry)| (entry.retention, entry.priority, entry.last_used));
        let mut bytes = self.bytes;
        let mut evictions = Vec::new();
        for (key, entry) in entries {
            if bytes <= target {
                break;
            }
            bytes = bytes.saturating_sub(entry.bytes);
            evictions.push(key.clone());
        }
        evictions
    }
}

#[cfg(test)]
pub(super) fn bitmap_cache_eviction_plan(
    entries: impl IntoIterator<
        Item = (
            D2dBitmapCacheKey,
            crate::memory::RetentionClass,
            crate::memory::CachePriority,
            u64,
            usize,
        ),
    >,
    mut bytes: usize,
    budget_bytes: usize,
) -> Vec<D2dBitmapCacheKey> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries
        .sort_by_key(|(_, retention, priority, last_used, _)| (*retention, *priority, *last_used));
    let mut evictions = Vec::new();
    for (key, _, _, _, entry_bytes) in entries {
        if bytes <= budget_bytes {
            break;
        }
        bytes = bytes.saturating_sub(entry_bytes);
        evictions.push(key);
    }
    evictions
}

pub(super) struct D2dCompositingLayer {
    pub(super) content_signature: Option<u64>,
    pub(super) shadow: Option<crate::core::ShadowStyle>,
    pub(super) background: CompositingLayerBackground,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) bitmap: ID2D1Bitmap1,
    pub(super) commands: Vec<ScenePrimitive>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct D2dOverlayBrushCacheKey {
    pub(super) rect: UiRect,
    pub(super) style_signature: u64,
}

pub(super) struct D2dOverlayBrushSet {
    pub(super) linear: Vec<ID2D1LinearGradientBrush>,
    pub(super) radial: Vec<ID2D1RadialGradientBrush>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum D2dBitmapCacheKey {
    Image {
        source: UiImageSource,
        fit: ImageFit,
        width: i32,
        height: i32,
    },
    Icon {
        key: &'static str,
        color: u32,
        alpha: u8,
        width: i32,
        height: i32,
    },
    BackdropBlur {
        signature: u64,
        width: i32,
        height: i32,
    },
    BackdropBlurPath {
        signature: u64,
        width: i32,
        height: i32,
    },
    StaticLayer {
        raster_key: String,
        id: UiId,
        spec_signature: u64,
        child_signature: u64,
        width: i32,
        height: i32,
    },
}

impl D2dBitmapCacheKey {
    pub(super) fn estimated_bytes(&self) -> usize {
        let (width, height) = match self {
            Self::Image { width, height, .. }
            | Self::Icon { width, height, .. }
            | Self::BackdropBlur { width, height, .. }
            | Self::BackdropBlurPath { width, height, .. }
            | Self::StaticLayer { width, height, .. } => (*width, *height),
        };
        (width.max(1) as usize)
            .saturating_mul(height.max(1) as usize)
            .saturating_mul(4)
    }
}
