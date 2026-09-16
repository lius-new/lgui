use std::{
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use windows::Win32::Graphics::Gdi::HDC;

use super::image::draw_image;
use crate::render_trace;
use lgui_core::core::{
    RasterCachePolicy, RenderPhase, ScenePrimitive, ScrollRasterSpec, StaticLayerBackground,
    StaticLayerSource, StaticLayerSpec, UiId, UiRect, VisualStyle,
};
use lgui_core::renderer::{StaticLayerMemoryCachePrefixStats, StaticLayerMemoryCacheStats};

fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

fn static_layer_cache() -> &'static Mutex<StaticLayerMemoryCache> {
    static CACHE: OnceLock<Mutex<StaticLayerMemoryCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(StaticLayerMemoryCache::default()))
}

#[derive(Clone)]
struct StaticLayerBitmap {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

struct StaticLayerMemoryEntry {
    id: String,
    last_used: u64,
    bytes: usize,
    retention: lgui_core::memory::RetentionClass,
    priority: lgui_core::memory::CachePriority,
    bitmap: StaticLayerBitmap,
}

struct StaticLayerMemoryCache {
    entries: HashMap<String, StaticLayerMemoryEntry>,
    bytes: usize,
    tick: u64,
    budget_bytes: usize,
    hits: u64,
    misses: u64,
    stores: u64,
    evictions: u64,
    governor_budget_bytes: usize,
}

impl Default for StaticLayerMemoryCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            tick: 0,
            budget_bytes: 0,
            hits: 0,
            misses: 0,
            stores: 0,
            evictions: 0,
            governor_budget_bytes: 0,
        }
    }
}

impl StaticLayerBitmap {
    fn byte_len(&self) -> usize {
        self.pixels.len()
    }
}

impl StaticLayerMemoryCache {
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

    fn store(
        &mut self,
        id: &str,
        key: String,
        bitmap: StaticLayerBitmap,
        budget_bytes: usize,
        policy: RasterCachePolicy,
    ) -> bool {
        let bytes = bitmap.byte_len();
        self.budget_bytes = budget_bytes.min(self.governor_budget_bytes);
        self.stores = self.stores.saturating_add(1);
        if bytes > self.budget_bytes {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        let entry = StaticLayerMemoryEntry {
            id: id.to_string(),
            last_used: self.next_tick(),
            bytes,
            retention: policy.retention().unwrap_or_default(),
            priority: policy.priority().unwrap_or_default(),
            bitmap,
        };
        self.bytes = self.bytes.saturating_add(entry.bytes);
        self.entries.insert(key, entry);
        self.evict_to_budget();
        true
    }

    fn evict_to_budget(&mut self) {
        while self.bytes > self.budget_bytes {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| (entry.retention, entry.priority, entry.last_used))
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

pub(crate) fn trim_static_layer_memory_cache(target_bytes: usize) -> usize {
    let mut cache = static_layer_cache()
        .lock()
        .expect("static layer cache poisoned");
    let before = cache.bytes;
    cache.budget_bytes = cache.budget_bytes.min(target_bytes);
    cache.evict_to_budget();
    before.saturating_sub(cache.bytes)
}

pub(crate) fn set_static_layer_memory_cache_budget(budget_bytes: usize) {
    let mut cache = static_layer_cache()
        .lock()
        .expect("static layer cache poisoned");
    cache.governor_budget_bytes = budget_bytes;
    cache.budget_bytes = cache.budget_bytes.min(cache.governor_budget_bytes);
    cache.evict_to_budget();
}

#[doc(hidden)]
pub fn static_layer_cache_key(
    id: &UiId,
    spec: &StaticLayerSpec,
    width: i32,
    height: i32,
    child_signature: u64,
) -> String {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    child_signature.hash(&mut hasher);
    spec.cache_signature().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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

#[cfg(test)]
#[path = "static_layer_test.rs"]
mod tests;
