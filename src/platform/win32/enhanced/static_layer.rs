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

#[derive(Clone, Debug, Default)]
pub struct StaticLayerMemoryCacheStats {
    pub entry_count: usize,
    pub bytes: usize,
    pub budget_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub stores: u64,
    pub evictions: u64,
}

#[derive(Clone, Debug, Default)]
pub struct StaticLayerMemoryCachePrefixStats {
    pub entry_count: usize,
    pub bytes: usize,
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

pub fn draw_static_layer<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    child_signature: u64,
) {
    let start = Instant::now();
    let draw_rect = rect.translate(spec.offset_x, spec.offset_y);
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let key = static_layer_raster_cache::cache_key(id, spec, width, height, child_signature);
    let use_memory_cache = matches!(
        spec.cache_policy,
        StaticLayerCachePolicy::Memory | StaticLayerCachePolicy::MemoryAndDisk
    );
    let use_disk_cache = matches!(spec.cache_policy, StaticLayerCachePolicy::MemoryAndDisk);

    if commands.is_empty() && !use_memory_cache && !use_disk_cache {
        match &spec.source {
            StaticLayerSource::BakedAsset { key, fit }
            | StaticLayerSource::Hybrid {
                baked_base: Some(key),
                fit,
            } => {
                draw_image(hdc, draw_rect, key, *fit);
                trace_duration("gdi.static_layer.baked", start.elapsed());
                return;
            }
            StaticLayerSource::RuntimeGenerated
            | StaticLayerSource::Hybrid {
                baked_base: None, ..
            } => {}
        }
    }

    if use_memory_cache && blit_cached_static_layer::<B>(hdc, draw_rect, None, &key, spec.opacity) {
        trace_duration("gdi.static_layer.memory_hit", start.elapsed());
        return;
    }

    if use_disk_cache
        && static_layer_raster_cache::load(&key).is_some_and(|raster| {
            let bitmap = StaticLayerBitmap::from(raster);
            store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
            blit_cached_static_layer::<B>(hdc, draw_rect, None, &key, spec.opacity)
        })
    {
        trace_duration("gdi.static_layer.disk_hit", start.elapsed());
        return;
    }

    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, rect, spec, commands) else {
        return;
    };
    if use_disk_cache {
        static_layer_raster_cache::store(&key, &StaticLayerRaster::from(&bitmap));
    }
    if use_memory_cache {
        store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
        let _ = blit_cached_static_layer::<B>(hdc, draw_rect, None, &key, spec.opacity);
    } else {
        B::blit_premultiplied_bgra_alpha(
            hdc,
            draw_rect,
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            spec.opacity,
        );
    }
    trace_duration("gdi.static_layer.generate", start.elapsed());
}

pub fn draw_static_layer_region<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    rect: UiRect,
    clip: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    child_signature: u64,
) -> bool {
    let start = Instant::now();
    let draw_rect = rect.translate(spec.offset_x, spec.offset_y);
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let key = static_layer_raster_cache::cache_key(id, spec, width, height, child_signature);
    let use_memory_cache = matches!(
        spec.cache_policy,
        StaticLayerCachePolicy::Memory | StaticLayerCachePolicy::MemoryAndDisk
    );
    let use_disk_cache = matches!(spec.cache_policy, StaticLayerCachePolicy::MemoryAndDisk);
    if use_memory_cache
        && blit_cached_static_layer::<B>(hdc, draw_rect, Some(clip), &key, spec.opacity)
    {
        trace_duration("gdi.static_layer.region_memory_hit", start.elapsed());
        return true;
    }

    if use_disk_cache
        && static_layer_raster_cache::load(&key).is_some_and(|raster| {
            let bitmap = StaticLayerBitmap::from(raster);
            store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
            blit_cached_static_layer::<B>(hdc, draw_rect, Some(clip), &key, spec.opacity)
        })
    {
        trace_duration("gdi.static_layer.region_disk_hit", start.elapsed());
        return true;
    }

    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, rect, spec, commands) else {
        return false;
    };
    if use_disk_cache {
        static_layer_raster_cache::store(&key, &StaticLayerRaster::from(&bitmap));
    }
    if use_memory_cache {
        store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
        let hit = blit_cached_static_layer::<B>(hdc, draw_rect, Some(clip), &key, spec.opacity);
        if hit {
            trace_duration("gdi.static_layer.region_generate", start.elapsed());
        }
        hit
    } else {
        let Some(dest) = draw_rect.intersect(clip) else {
            return true;
        };
        let source = UiRect::new(
            dest.left - draw_rect.left,
            dest.top - draw_rect.top,
            dest.right - draw_rect.left,
            dest.bottom - draw_rect.top,
        );
        B::blit_cached_premultiplied_bgra_region_alpha(
            hdc,
            &key,
            dest,
            source,
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            spec.opacity,
        );
        trace_duration("gdi.static_layer.region_generate", start.elapsed());
        true
    }
}

pub fn draw_scroll_raster<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    _child_signature: u64,
) {
    let frame_start = Instant::now();
    for tile_index in &spec.visible_tiles {
        draw_scroll_raster_visible_tile::<B>(
            hdc,
            id,
            viewport,
            spec,
            commands,
            *tile_index,
            _child_signature,
        );
    }

    let mut warmed = 0usize;
    for tile_index in &spec.prefetch_tiles {
        if warmed >= spec.max_prefetch_tiles_per_frame {
            break;
        }
        if frame_start.elapsed().as_millis() >= u128::from(spec.max_prefetch_ms_per_frame) {
            break;
        }
        if scroll_raster_tile_cached(id, viewport, spec, *tile_index, _child_signature) {
            continue;
        }
        if frame_start.elapsed().as_millis() >= u128::from(spec.max_prefetch_ms_per_frame) {
            break;
        }
        if warm_scroll_raster_tile::<B>(
            hdc,
            id,
            viewport,
            spec,
            commands,
            *tile_index,
            _child_signature,
        ) {
            warmed = warmed.saturating_add(1);
        }
    }
}

fn draw_scroll_raster_visible_tile<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    tile_index: usize,
    child_signature: u64,
) {
    let tile_rect = scroll_raster_tile_rect(viewport, spec, tile_index);
    let tile_id = scroll_raster_tile_id(id, tile_index);
    let tile_spec = scroll_raster_tile_spec(spec, 0xFF);
    let key = scroll_raster_tile_cache_key(&tile_id, tile_rect, &tile_spec, spec, child_signature);
    let draw_rect = tile_rect.translate(tile_spec.offset_x, tile_spec.offset_y);
    if blit_cached_static_layer::<B>(hdc, draw_rect, Some(viewport), &key, tile_spec.opacity) {
        return;
    }
    let tile_commands = commands_for_tile(id, spec, commands, tile_rect);
    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, tile_rect, &tile_spec, &tile_commands)
    else {
        return;
    };
    store_static_layer_memory(
        tile_id.as_str(),
        key.clone(),
        bitmap,
        tile_spec.memory_budget_bytes,
    );
    let _ = blit_cached_static_layer::<B>(hdc, draw_rect, Some(viewport), &key, tile_spec.opacity);
}

fn warm_scroll_raster_tile<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    tile_index: usize,
    child_signature: u64,
) -> bool {
    let start = Instant::now();
    let tile_rect = scroll_raster_tile_rect(viewport, spec, tile_index);
    let tile_id = scroll_raster_tile_id(id, tile_index);
    let tile_spec = scroll_raster_tile_spec(spec, 0xFF);
    let key = scroll_raster_tile_cache_key(&tile_id, tile_rect, &tile_spec, spec, child_signature);
    if static_layer_memory_contains_key(&key) {
        return false;
    }
    let tile_commands = commands_for_tile(id, spec, commands, tile_rect);
    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, tile_rect, &tile_spec, &tile_commands)
    else {
        return false;
    };
    store_static_layer_memory(tile_id.as_str(), key, bitmap, tile_spec.memory_budget_bytes);
    trace_duration("gdi.scroll_raster.prefetch_generate", start.elapsed());
    true
}

fn scroll_raster_tile_rect(viewport: UiRect, spec: &ScrollRasterSpec, tile_index: usize) -> UiRect {
    let tile_height = spec.tile_height_px.max(1);
    let top = viewport.top + (tile_index as i32 * tile_height);
    let bottom = (top + tile_height).min(viewport.top + spec.content_height);
    UiRect::new(viewport.left, top, viewport.right, bottom.max(top + 1))
}

fn scroll_raster_tile_id(id: &UiId, tile_index: usize) -> UiId {
    UiId::owned(format!("{}.tile.{tile_index}", id.as_str()))
}

fn scroll_raster_tile_spec(spec: &ScrollRasterSpec, opacity: u8) -> StaticLayerSpec {
    let background = if spec.background_fill.is_some() {
        StaticLayerBackground::Opaque
    } else {
        StaticLayerBackground::Transparent
    };
    StaticLayerSpec::new(StaticLayerSource::runtime())
        .cache_policy(StaticLayerCachePolicy::Memory)
        .memory_budget_bytes(spec.memory_budget_bytes)
        .paint_offset(0, -spec.scroll_y)
        .opacity(opacity as f32 / 255.0)
        .revision("scroll-raster-height-tile-v1")
        .background(background)
}

fn scroll_raster_tile_cached(
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    tile_index: usize,
    child_signature: u64,
) -> bool {
    let tile_rect = scroll_raster_tile_rect(viewport, spec, tile_index);
    let tile_id = scroll_raster_tile_id(id, tile_index);
    let tile_spec = scroll_raster_tile_spec(spec, 0xFF);
    let key = scroll_raster_tile_cache_key(&tile_id, tile_rect, &tile_spec, spec, child_signature);
    static_layer_memory_contains_key(&key)
}

fn scroll_raster_tile_cache_key(
    id: &UiId,
    rect: UiRect,
    spec: &StaticLayerSpec,
    raster_spec: &ScrollRasterSpec,
    child_signature: u64,
) -> String {
    static_layer_raster_cache::cache_key(
        id,
        spec,
        rect.width().max(1),
        rect.height().max(1),
        scroll_raster_tile_signature(raster_spec, child_signature),
    )
}

fn scroll_raster_tile_signature(spec: &ScrollRasterSpec, child_signature: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    spec.cache_epoch.hash(&mut hasher);
    spec.tile_height_px.hash(&mut hasher);
    spec.content_height.hash(&mut hasher);
    spec.background_fill.hash(&mut hasher);
    child_signature.hash(&mut hasher);
    hasher.finish()
}

fn commands_for_tile(
    id: &UiId,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    tile_rect: UiRect,
) -> Vec<ScenePrimitive> {
    let mut tile_commands = Vec::new();
    if let Some(fill) = spec.background_fill {
        tile_commands.push(ScenePrimitive::Rect {
            id: UiId::owned(format!("{}.raster.background", id.as_str())),
            rect: tile_rect,
            style: VisualStyle::filled(fill),
            phase: RenderPhase::Content,
        });
    }
    tile_commands.extend(
        commands
            .iter()
            .filter(|command| command.rect().intersect(tile_rect).is_some())
            .cloned(),
    );
    tile_commands
}

fn render_static_layer_bitmap<B: StaticLayerDrawBackend>(
    hdc: HDC,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
) -> Option<StaticLayerBitmap> {
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    B::with_dib_section(hdc, width, height, |memory_dc, bits| {
        B::clear_alpha_buffer(bits, width, height);
        let local_rect = UiRect::new(0, 0, width, height);
        match &spec.source {
            StaticLayerSource::BakedAsset { key, fit } => {
                draw_image(memory_dc, local_rect, key, *fit);
            }
            StaticLayerSource::RuntimeGenerated => {}
            StaticLayerSource::Hybrid { baked_base, fit } => {
                if let Some(key) = baked_base {
                    draw_image(memory_dc, local_rect, key, *fit);
                }
            }
        }

        for command in commands {
            let local = translate_command(command, -rect.left, -rect.top);
            B::draw_command(memory_dc, &local);
        }
        B::prepare_alpha_buffer(bits, width, height, spec.background);
        let len = (width * height * 4) as usize;
        let pixels = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len) }.to_vec();
        Some(StaticLayerBitmap {
            width,
            height,
            pixels,
        })
    })
    .flatten()
}

impl From<StaticLayerRaster> for StaticLayerBitmap {
    fn from(raster: StaticLayerRaster) -> Self {
        Self {
            width: raster.width,
            height: raster.height,
            pixels: raster.premultiplied_bgra,
        }
    }
}

impl From<&StaticLayerBitmap> for StaticLayerRaster {
    fn from(bitmap: &StaticLayerBitmap) -> Self {
        Self {
            width: bitmap.width,
            height: bitmap.height,
            premultiplied_bgra: bitmap.pixels.clone(),
        }
    }
}

fn translate_command(command: &ScenePrimitive, dx: i32, dy: i32) -> ScenePrimitive {
    let translate_rect = |rect: UiRect| {
        UiRect::new(
            rect.left + dx,
            rect.top + dy,
            rect.right + dx,
            rect.bottom + dy,
        )
    };
    let translate_point =
        |point: lgui::core::Point| lgui::core::Point::new(point.x + dx, point.y + dy);
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Rect {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Ellipse {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => ScenePrimitive::Text {
            id: id.clone(),
            rect: translate_rect(*rect),
            text: text.clone(),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Custom {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => ScenePrimitive::Line {
            id: id.clone(),
            start: translate_point(*start),
            end: translate_point(*end),
            stroke: *stroke,
            phase: *phase,
        },
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::Path {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Image {
            id,
            rect,
            source,
            fit,
            phase,
        } => ScenePrimitive::Image {
            id: id.clone(),
            rect: translate_rect(*rect),
            source: source.clone(),
            fit: *fit,
            phase: *phase,
        },
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Icon {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => ScenePrimitive::Glow {
            id: id.clone(),
            rect: translate_rect(*rect),
            color: *color,
            alpha: *alpha,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::BackdropBlur {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::BackdropBlurPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Overlay {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: style.clone(),
            phase: *phase,
        },
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::StaticLayer {
            id: id.clone(),
            rect: translate_rect(*rect),
            spec: spec.clone(),
            commands: commands.clone(),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ScrollRaster {
            id: id.clone(),
            viewport: translate_rect(*viewport),
            spec: spec.clone(),
            commands: commands
                .iter()
                .map(|command| translate_command(command, dx, dy))
                .collect(),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::Clip {
            id,
            rect,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::Clip {
            id: id.clone(),
            rect: translate_rect(*rect),
            commands: commands
                .iter()
                .map(|command| translate_command(command, dx, dy))
                .collect(),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ClipPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            commands: commands
                .iter()
                .map(|command| translate_command(command, dx, dy))
                .collect(),
            child_signature: *child_signature,
            phase: *phase,
        },
    }
}

fn translate_path(path: &lgui::core::UiPath, dx: i32, dy: i32) -> lgui::core::UiPath {
    use lgui::core::{Point, UiPath, UiPathCommand};

    let translate = |point: Point| Point::new(point.x + dx, point.y + dy);
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(translate(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(translate(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: translate(control),
            to: translate(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: translate(control1),
            control2: translate(control2),
            to: translate(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}

fn blit_cached_static_layer<B: StaticLayerDrawBackend>(
    hdc: HDC,
    rect: UiRect,
    clip: Option<UiRect>,
    key: &str,
    opacity: u8,
) -> bool {
    let mut cache = static_layer_cache()
        .lock()
        .expect("static layer cache poisoned");
    let Some(bitmap) = cache.touch(key) else {
        return false;
    };
    if let Some(clip) = clip {
        let Some(dest) = rect.intersect(clip) else {
            return true;
        };
        let source = UiRect::new(
            dest.left - rect.left,
            dest.top - rect.top,
            dest.right - rect.left,
            dest.bottom - rect.top,
        );
        B::blit_cached_premultiplied_bgra_region_alpha(
            hdc,
            key,
            dest,
            source,
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            opacity,
        );
    } else {
        B::blit_cached_premultiplied_bgra_region_alpha(
            hdc,
            key,
            rect,
            UiRect::new(0, 0, bitmap.width, bitmap.height),
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            opacity,
        );
    }
    true
}

fn store_static_layer_memory(
    id: &str,
    key: String,
    bitmap: StaticLayerBitmap,
    budget_bytes: usize,
) {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .store(id, key, bitmap, budget_bytes);
}

fn trace_duration(label: &str, duration: Duration) {
    if render_trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}
