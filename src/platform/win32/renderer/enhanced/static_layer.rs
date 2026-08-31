use std::{
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use windows::Win32::Graphics::Gdi::HDC;

use super::{
    image::draw_image,
    static_layer_raster_cache::{self, StaticLayerRaster},
};
use lgui::core::{
    RenderPhase, ScenePrimitive, ScrollRasterSpec, StaticLayerBackground, StaticLayerCachePolicy,
    StaticLayerSource, StaticLayerSpec, UiId, UiRect, VisualStyle,
};
use lgui::platform::win32::render_trace;
use lgui::renderer::{StaticLayerMemoryCachePrefixStats, StaticLayerMemoryCacheStats};

fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

fn static_layer_cache() -> &'static Mutex<StaticLayerMemoryCache> {
    static CACHE: OnceLock<Mutex<StaticLayerMemoryCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StaticLayerMemoryCache::default()))
}

struct StaticLayerBitmap {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

struct StaticLayerMemoryEntry {
    id: String,
    last_used: u64,
    bytes: usize,
    bitmap: StaticLayerBitmap,
}

#[derive(Default)]
struct StaticLayerMemoryCache {
    entries: HashMap<String, StaticLayerMemoryEntry>,
    bytes: usize,
    tick: u64,
    budget_bytes: usize,
    hits: u64,
    misses: u64,
    stores: u64,
    evictions: u64,
}

impl StaticLayerBitmap {
    fn byte_len(&self) -> usize {
        self.pixels.len()
    }
}

impl StaticLayerMemoryCache {
    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.tick = 0;
        self.budget_bytes = 0;
        self.hits = 0;
        self.misses = 0;
        self.stores = 0;
        self.evictions = 0;
    }

    fn stats(&self) -> StaticLayerMemoryCacheStats {
        StaticLayerMemoryCacheStats {
            entry_count: self.entries.len(),
            bytes: self.bytes,
            budget_bytes: self.budget_bytes,
            hits: self.hits,
            misses: self.misses,
            stores: self.stores,
            evictions: self.evictions,
        }
    }

    fn prefix_stats(&self, id_prefix: &str) -> StaticLayerMemoryCachePrefixStats {
        let mut stats = StaticLayerMemoryCachePrefixStats::default();
        for entry in self.entries.values() {
            if entry.id.starts_with(id_prefix) {
                stats.entry_count = stats.entry_count.saturating_add(1);
                stats.bytes = stats.bytes.saturating_add(entry.bytes);
            }
        }
        stats
    }

    fn prefix_entry_ids(&self, id_prefix: &str) -> Vec<String> {
        let mut ids: Vec<String> = self
            .entries
            .values()
            .filter(|entry| entry.id.starts_with(id_prefix))
            .map(|entry| entry.id.clone())
            .collect();
        ids.sort();
        ids
    }

    fn retain_entries(&mut self, mut keep: impl FnMut(&StaticLayerMemoryEntry) -> bool) {
        let mut removed_bytes = 0usize;
        self.entries.retain(|_, entry| {
            let retain = keep(entry);
            if !retain {
                removed_bytes = removed_bytes.saturating_add(entry.bytes);
            }
            retain
        });
        self.bytes = self.bytes.saturating_sub(removed_bytes);
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }

    fn touch(&mut self, key: &str) -> Option<&StaticLayerBitmap> {
        let tick = self.next_tick();
        if !self.entries.contains_key(key) {
            self.misses = self.misses.saturating_add(1);
            return None;
        }
        self.hits = self.hits.saturating_add(1);
        let entry = self
            .entries
            .get_mut(key)
            .expect("static layer cache key vanished");
        entry.last_used = tick;
        Some(&entry.bitmap)
    }

    fn store(&mut self, id: &str, key: String, bitmap: StaticLayerBitmap, budget_bytes: usize) {
        let bytes = bitmap.byte_len();
        self.budget_bytes = budget_bytes.max(bytes).max(1);
        self.stores = self.stores.saturating_add(1);
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        let entry = StaticLayerMemoryEntry {
            id: id.to_string(),
            last_used: self.next_tick(),
            bytes,
            bitmap,
        };
        self.bytes = self.bytes.saturating_add(entry.bytes);
        self.entries.insert(key, entry);
        self.evict_to_budget();
    }

    fn evict_to_budget(&mut self) {
        while self.bytes > self.budget_bytes && self.entries.len() > 1 {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest_key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }
}

pub fn clear_static_layer_memory_cache() {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .clear();
}

pub fn static_layer_memory_cache_stats() -> StaticLayerMemoryCacheStats {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .stats()
}

pub fn static_layer_memory_cache_stats_for_prefix(
    id_prefix: &str,
) -> StaticLayerMemoryCachePrefixStats {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .prefix_stats(id_prefix)
}

pub fn static_layer_memory_cache_entry_ids_for_prefix(id_prefix: &str) -> Vec<String> {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .prefix_entry_ids(id_prefix)
}

pub fn clear_scroll_raster_memory_cache() {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .retain_entries(|entry| !entry.id.contains(".raster.tile."));
}

fn static_layer_memory_contains_key(key: &str) -> bool {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .entries
        .contains_key(key)
}

pub trait StaticLayerDrawBackend {
    fn draw_command(hdc: HDC, command: &ScenePrimitive);
    fn blit_premultiplied_bgra(hdc: HDC, rect: UiRect, width: i32, height: i32, pixels: &[u8]);
    fn blit_premultiplied_bgra_alpha(
        hdc: HDC,
        rect: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    );
    fn blit_premultiplied_bgra_region(
        hdc: HDC,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
    );
    fn blit_premultiplied_bgra_region_alpha(
        hdc: HDC,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    );
    fn blit_cached_premultiplied_bgra_region_alpha(
        hdc: HDC,
        cache_key: &str,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    ) {
        let _ = cache_key;
        Self::blit_premultiplied_bgra_region_alpha(
            hdc,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        );
    }
    fn with_dib_section<T>(
        hdc: HDC,
        width: i32,
        height: i32,
        draw: impl FnOnce(HDC, *mut std::ffi::c_void) -> T,
    ) -> Option<T>;
    fn clear_alpha_buffer(bits: *mut std::ffi::c_void, width: i32, height: i32);
    fn prepare_alpha_buffer(
        bits: *mut std::ffi::c_void,
        width: i32,
        height: i32,
        background: StaticLayerBackground,
    );
}

mod draw;
mod raster;
mod scroll;

pub use draw::{draw_static_layer, draw_static_layer_region};
pub use scroll::draw_scroll_raster;
