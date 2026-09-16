use super::{memory::estimate_scene_commands_bytes, primitive::*, *};

const SCROLL_RASTER_COMMAND_CACHE_LIMIT: usize = 8;

#[derive(Clone, Hash, PartialEq, Eq)]
struct ScrollRasterCommandCacheKey {
    id: String,
    cache_epoch: u64,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    content_height: u32,
}

#[derive(Clone)]
struct ScrollRasterCommandSnapshot {
    commands: Vec<ScenePrimitive>,
    child_signature: u64,
    last_used: u64,
    bytes: usize,
}

struct ScrollRasterCommandCache {
    entries: HashMap<ScrollRasterCommandCacheKey, ScrollRasterCommandSnapshot>,
    tick: u64,
    bytes: usize,
    budget_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Default for ScrollRasterCommandCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            tick: 0,
            bytes: 0,
            budget_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
}

fn scroll_raster_command_cache() -> &'static Mutex<ScrollRasterCommandCache> {
    static CACHE: OnceLock<Mutex<ScrollRasterCommandCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ScrollRasterCommandCache::default()))
}

pub(crate) fn scroll_raster_command_cache_usage() -> crate::memory::CacheUsage {
    let cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    crate::memory::CacheUsage {
        rebuildable_bytes: cache.bytes,
        cpu_bytes: cache.bytes,
        entries: cache.entries.len(),
        hits: cache.hits,
        misses: cache.misses,
        evictions: cache.evictions,
        largest_entry_bytes: cache
            .entries
            .values()
            .map(|entry| entry.bytes)
            .max()
            .unwrap_or(0),
        ..Default::default()
    }
}

pub(crate) fn trim_scroll_raster_command_cache(target_bytes: usize) -> usize {
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    let before = cache.bytes;
    evict_scroll_raster_commands(&mut cache, target_bytes, SCROLL_RASTER_COMMAND_CACHE_LIMIT);
    before.saturating_sub(cache.bytes)
}

pub(crate) fn set_scroll_raster_command_cache_budget(budget_bytes: usize) {
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    cache.budget_bytes = budget_bytes;
    let budget = cache.budget_bytes;
    evict_scroll_raster_commands(&mut cache, budget, SCROLL_RASTER_COMMAND_CACHE_LIMIT);
}

pub(super) fn load_scroll_raster_command_snapshot(
    node: &UiNode,
    spec: &ScrollRasterSpec,
) -> Option<(Vec<ScenePrimitive>, u64)> {
    let key = scroll_raster_command_cache_key(node, spec);
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    cache.tick = cache.tick.saturating_add(1);
    let tick = cache.tick;
    if !cache.entries.contains_key(&key) {
        cache.misses = cache.misses.saturating_add(1);
        return None;
    }
    cache.hits = cache.hits.saturating_add(1);
    let snapshot = cache
        .entries
        .get_mut(&key)
        .expect("scroll raster command cache key vanished");
    snapshot.last_used = tick;
    Some((snapshot.commands.clone(), snapshot.child_signature))
}

pub fn scroll_raster_command_snapshot_exists(
    id: &UiId,
    rect: UiRect,
    spec: &ScrollRasterSpec,
) -> bool {
    let key = scroll_raster_command_cache_key_for(id, rect, spec);
    scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned")
        .entries
        .contains_key(&key)
}

pub(super) fn store_scroll_raster_command_snapshot(
    node: &UiNode,
    spec: &ScrollRasterSpec,
    commands: Vec<ScenePrimitive>,
    child_signature: u64,
) {
    let key = scroll_raster_command_cache_key(node, spec);
    let bytes = estimate_scene_commands_bytes(&commands);
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    if bytes > cache.budget_bytes {
        return;
    }
    cache.tick = cache.tick.saturating_add(1);
    let tick = cache.tick;
    if let Some(previous) = cache.entries.remove(&key) {
        cache.bytes = cache.bytes.saturating_sub(previous.bytes);
    }
    cache.bytes = cache.bytes.saturating_add(bytes);
    cache.entries.insert(
        key,
        ScrollRasterCommandSnapshot {
            commands,
            child_signature,
            last_used: tick,
            bytes,
        },
    );
    let budget = cache.budget_bytes;
    evict_scroll_raster_commands(&mut cache, budget, SCROLL_RASTER_COMMAND_CACHE_LIMIT);
}

fn evict_scroll_raster_commands(
    cache: &mut ScrollRasterCommandCache,
    target_bytes: usize,
    target_entries: usize,
) {
    while cache.bytes > target_bytes || cache.entries.len() > target_entries {
        let Some(oldest_key) = cache
            .entries
            .iter()
            .min_by_key(|(_, snapshot)| snapshot.last_used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(entry) = cache.entries.remove(&oldest_key) {
            cache.bytes = cache.bytes.saturating_sub(entry.bytes);
            cache.evictions = cache.evictions.saturating_add(1);
        }
    }
}

fn scroll_raster_command_cache_key(
    node: &UiNode,
    spec: &ScrollRasterSpec,
) -> ScrollRasterCommandCacheKey {
    scroll_raster_command_cache_key_for(&node.id, node.layout_rect, spec)
}

fn scroll_raster_command_cache_key_for(
    id: &UiId,
    rect: UiRect,
    spec: &ScrollRasterSpec,
) -> ScrollRasterCommandCacheKey {
    ScrollRasterCommandCacheKey {
        id: id.as_str().to_string(),
        cache_epoch: spec.cache_epoch,
        left: normalized_f32_bits(rect.left),
        top: normalized_f32_bits(rect.top),
        right: normalized_f32_bits(rect.right),
        bottom: normalized_f32_bits(rect.bottom),
        content_height: normalized_f32_bits(spec.content_height),
    }
}
