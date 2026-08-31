pub struct D2dRenderer {
    context: ID2D1DeviceContext,
    dwrite_factory: IDWriteFactory,
    scene_bitmap: ID2D1Bitmap1,
    bitmap_cache: D2dBitmapCache,
    overlay_brush_cache: HashMap<D2dOverlayBrushCacheKey, D2dOverlayBrushSet>,
    frame_bitmap_cache: HashMap<D2dBitmapCacheKey, ID2D1Bitmap1>,
    compositing_layers: HashMap<UiId, D2dCompositingLayer>,
}

const D2D_BITMAP_CACHE_MIN_BUDGET_BYTES: usize = 32 * 1024 * 1024;
const D2D_BITMAP_CACHE_VIEWPORT_MULTIPLIER: usize = 4;

fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

fn raster_size(rect: UiRect) -> (i32, i32) {
    (raster_length(rect.width()), raster_length(rect.height()))
}

fn d2d_bitmap_cache_budget(width: i32, height: i32) -> usize {
    (width.max(1) as usize)
        .saturating_mul(height.max(1) as usize)
        .saturating_mul(4)
        .saturating_mul(D2D_BITMAP_CACHE_VIEWPORT_MULTIPLIER)
        .max(D2D_BITMAP_CACHE_MIN_BUDGET_BYTES)
}

struct D2dBitmapCacheEntry {
    bitmap: ID2D1Bitmap1,
    bytes: usize,
    last_used: u64,
}

struct D2dBitmapCache {
    entries: HashMap<D2dBitmapCacheKey, D2dBitmapCacheEntry>,
    bytes: usize,
    tick: u64,
    budget_bytes: usize,
}

impl D2dBitmapCache {
    fn new(budget_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            tick: 0,
            budget_bytes: budget_bytes.max(1),
        }
    }

    fn retain_live(&mut self, live: &HashSet<D2dBitmapCacheKey>) {
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

    fn get(&mut self, key: &D2dBitmapCacheKey) -> Option<ID2D1Bitmap1> {
        self.tick = self.tick.saturating_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.tick;
        Some(entry.bitmap.clone())
    }

    fn insert(&mut self, key: D2dBitmapCacheKey, bitmap: ID2D1Bitmap1) {
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
            },
        );
    }

    fn evict_to_budget(&mut self) {
        let evictions = bitmap_cache_eviction_plan(
            self.entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry.last_used, entry.bytes)),
            self.bytes,
            self.budget_bytes,
        );
        for key in evictions {
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

fn bitmap_cache_eviction_plan(
    entries: impl IntoIterator<Item = (D2dBitmapCacheKey, u64, usize)>,
    mut bytes: usize,
    budget_bytes: usize,
) -> Vec<D2dBitmapCacheKey> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_by_key(|(_, last_used, _)| *last_used);
    let mut evictions = Vec::new();
    let mut remaining = entries.len();
    for (key, _, entry_bytes) in entries {
        if bytes <= budget_bytes || remaining <= 1 {
            break;
        }
        bytes = bytes.saturating_sub(entry_bytes);
        remaining -= 1;
        evictions.push(key);
    }
    evictions
}

struct D2dCompositingLayer {
    content_signature: Option<u64>,
    background: CompositingLayerBackground,
    width: i32,
    height: i32,
    bitmap: ID2D1Bitmap1,
    commands: Vec<ScenePrimitive>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct D2dOverlayBrushCacheKey {
    rect: UiRect,
    style_signature: u64,
}

struct D2dOverlayBrushSet {
    linear: Vec<ID2D1LinearGradientBrush>,
    radial: Vec<ID2D1RadialGradientBrush>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum D2dBitmapCacheKey {
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
    fn estimated_bytes(&self) -> usize {
        let (width, height) = match self {
            Self::Image { width, height, .. }
            | Self::Icon { width, height, .. }
            | Self::BackdropBlur { width, height, .. }
            | Self::StaticLayer { width, height, .. } => (*width, *height),
        };
        (width.max(1) as usize)
            .saturating_mul(height.max(1) as usize)
            .saturating_mul(4)
    }
}
