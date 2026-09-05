use super::*;

thread_local! {
    pub(super) static OVERLAY_CACHE: RefCell<crate::memory::LruCache<OverlayCacheKey, Vec<u8>>> = RefCell::new(
        crate::memory::LruCache::new(
            0,
            crate::memory::ResourceClass::Cache,
            gdi_telemetry(0).clone(),
        )
    );
    pub(super) static GDI_BITMAP_CACHE: RefCell<GdiBitmapCache> = RefCell::new(GdiBitmapCache::default());
    pub(super) static GDI_COMPOSITING_LAYER_SCOPE: Cell<u64> = const { Cell::new(0) };
    pub(super) static GDI_COMPOSITING_LAYERS: RefCell<HashMap<GdiCompositingLayerKey, GdiCompositingLayer>> = RefCell::new(HashMap::new());
    pub(super) static GDI_FRAME_BLIT_METRICS: RefCell<GdiFrameBlitMetrics> = RefCell::new(GdiFrameBlitMetrics::default());
    pub(super) static GDI_FONT_FAMILY_CACHE: RefCell<crate::memory::LruCache<(char, i32, i32), usize>> = RefCell::new(
        crate::memory::LruCache::new(
            0,
            crate::memory::ResourceClass::Cache,
            gdi_telemetry(2).clone(),
        )
    );
}

fn gdi_telemetry(index: usize) -> &'static crate::memory::CacheTelemetry {
    static TELEMETRY: std::sync::OnceLock<[crate::memory::CacheTelemetry; 4]> =
        std::sync::OnceLock::new();
    &TELEMETRY.get_or_init(Default::default)[index]
}

pub(super) fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

pub(super) fn round_coord(value: f32) -> i32 {
    value.round() as i32
}

pub(super) fn pixel_rect_outward(rect: UiRect) -> PhysicalRect {
    PhysicalRect::new(
        rect.left.floor() as i32,
        rect.top.floor() as i32,
        rect.right.ceil() as i32,
        rect.bottom.ceil() as i32,
    )
}

pub fn release_gdi_compositing_layer_scope(scope: u64) {
    GDI_COMPOSITING_LAYERS.with(|layers| {
        layers.borrow_mut().retain(|key, _| key.scope != scope);
    });
    publish_gdi_compositing_usage();
}

pub(crate) fn gdi_renderer_cache_usage() -> crate::memory::CacheUsage {
    let mut usage = crate::memory::CacheUsage::default();
    for index in 0..4 {
        usage.add_assign(gdi_telemetry(index).snapshot());
    }
    usage
}

pub(crate) fn trim_gdi_renderer_caches(target_bytes: usize) -> usize {
    let bitmap_target = target_bytes / 2;
    let overlay_target = target_bytes / 3;
    let font_target = target_bytes.saturating_sub(bitmap_target + overlay_target);
    OVERLAY_CACHE.with(|cache| cache.borrow_mut().trim_to(overlay_target))
        + GDI_BITMAP_CACHE.with(|cache| cache.borrow_mut().trim_to(bitmap_target))
        + GDI_FONT_FAMILY_CACHE.with(|cache| cache.borrow_mut().trim_to(font_target))
}

pub(crate) fn set_gdi_renderer_cache_budget(budget_bytes: usize) {
    let bitmap_budget = budget_bytes / 2;
    let overlay_budget = budget_bytes / 3;
    let font_budget = budget_bytes.saturating_sub(bitmap_budget + overlay_budget);
    OVERLAY_CACHE.with(|cache| cache.borrow_mut().set_budget(overlay_budget));
    GDI_BITMAP_CACHE.with(|cache| cache.borrow_mut().set_budget(bitmap_budget));
    GDI_FONT_FAMILY_CACHE.with(|cache| cache.borrow_mut().set_budget(font_budget));
}

pub(super) fn publish_gdi_compositing_usage() {
    let usage = GDI_COMPOSITING_LAYERS.with(|layers| {
        let layers = layers.borrow();
        let bytes = layers.values().fold(0usize, |total, layer| {
            total
                .saturating_add(layer.output.bytes)
                .saturating_add(layer.black.as_ref().map_or(0, |entry| entry.bytes))
                .saturating_add(layer.white.as_ref().map_or(0, |entry| entry.bytes))
        });
        crate::memory::CacheUsage {
            live_bytes: bytes,
            cpu_bytes: bytes,
            entries: layers.len(),
            largest_entry_bytes: layers
                .values()
                .map(|layer| {
                    layer
                        .output
                        .bytes
                        .saturating_add(layer.black.as_ref().map_or(0, |entry| entry.bytes))
                        .saturating_add(layer.white.as_ref().map_or(0, |entry| entry.bytes))
                })
                .max()
                .unwrap_or(0),
            ..Default::default()
        }
    });
    gdi_telemetry(3).publish(usage);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GdiFrameBlitSourceMetrics {
    pub blit_count: usize,
    pub bitblt_count: usize,
    pub alphablend_count: usize,
    pub fallback_count: usize,
    pub pixels: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GdiFrameBlitMetrics {
    pub blit_count: usize,
    pub bitblt_count: usize,
    pub alphablend_count: usize,
    pub fallback_count: usize,
    pub blit_pixels: u64,
    pub static_layer: GdiFrameBlitSourceMetrics,
    pub overlay: GdiFrameBlitSourceMetrics,
    pub backdrop: GdiFrameBlitSourceMetrics,
    pub custom: GdiFrameBlitSourceMetrics,
    pub other: GdiFrameBlitSourceMetrics,
}

pub fn reset_gdi_frame_blit_metrics() {
    GDI_FRAME_BLIT_METRICS.with(|metrics| {
        *metrics.borrow_mut() = GdiFrameBlitMetrics::default();
    });
}

pub fn take_gdi_frame_blit_metrics() -> GdiFrameBlitMetrics {
    GDI_FRAME_BLIT_METRICS.with(|metrics| {
        let snapshot = *metrics.borrow();
        *metrics.borrow_mut() = GdiFrameBlitMetrics::default();
        snapshot
    })
}

pub(super) fn record_gdi_frame_blit(
    source: GdiFrameBlitSource,
    kind: GdiFrameBlitKind,
    rect: UiRect,
) {
    let pixels =
        (rect.width().max(0.0).ceil() as u64).saturating_mul(rect.height().max(0.0).ceil() as u64);
    GDI_FRAME_BLIT_METRICS.with(|metrics| {
        let mut metrics_ref = metrics.borrow_mut();
        let metrics = &mut *metrics_ref;
        metrics.blit_count = metrics.blit_count.saturating_add(1);
        metrics.blit_pixels = metrics.blit_pixels.saturating_add(pixels);
        source_metrics_mut(metrics, source).record(kind, pixels);
        match kind {
            GdiFrameBlitKind::BitBlt => {
                metrics.bitblt_count = metrics.bitblt_count.saturating_add(1);
            }
            GdiFrameBlitKind::AlphaBlend => {
                metrics.alphablend_count = metrics.alphablend_count.saturating_add(1);
            }
            GdiFrameBlitKind::FallbackAlphaBlend => {
                metrics.alphablend_count = metrics.alphablend_count.saturating_add(1);
                metrics.fallback_count = metrics.fallback_count.saturating_add(1);
            }
        }
    });
}

pub(super) fn source_metrics_mut(
    metrics: &mut GdiFrameBlitMetrics,
    source: GdiFrameBlitSource,
) -> &mut GdiFrameBlitSourceMetrics {
    match source {
        GdiFrameBlitSource::StaticLayer => &mut metrics.static_layer,
        GdiFrameBlitSource::Overlay => &mut metrics.overlay,
        GdiFrameBlitSource::Backdrop => &mut metrics.backdrop,
        GdiFrameBlitSource::Custom => &mut metrics.custom,
        GdiFrameBlitSource::Other => &mut metrics.other,
    }
}

impl GdiFrameBlitSourceMetrics {
    fn record(&mut self, kind: GdiFrameBlitKind, pixels: u64) {
        self.blit_count = self.blit_count.saturating_add(1);
        self.pixels = self.pixels.saturating_add(pixels);
        match kind {
            GdiFrameBlitKind::BitBlt => {
                self.bitblt_count = self.bitblt_count.saturating_add(1);
            }
            GdiFrameBlitKind::AlphaBlend => {
                self.alphablend_count = self.alphablend_count.saturating_add(1);
            }
            GdiFrameBlitKind::FallbackAlphaBlend => {
                self.alphablend_count = self.alphablend_count.saturating_add(1);
                self.fallback_count = self.fallback_count.saturating_add(1);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum GdiFrameBlitKind {
    BitBlt,
    AlphaBlend,
    FallbackAlphaBlend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum GdiFrameBlitSource {
    StaticLayer,
    Overlay,
    Backdrop,
    Custom,
    Other,
}

pub(super) struct GdiBitmapCache {
    pub(super) entries: HashMap<String, GdiBitmapEntry>,
    pub(super) bytes: usize,
    pub(super) tick: u64,
    pub(super) budget_bytes: usize,
    pub(super) hits: u64,
    pub(super) misses: u64,
    pub(super) evictions: u64,
}

impl Default for GdiBitmapCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            tick: 0,
            budget_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
}

pub(super) struct GdiBitmapEntry {
    pub(super) memory_dc: HDC,
    pub(super) bitmap: HBITMAP,
    pub(super) old_bitmap: HGDIOBJ,
    pub(super) bits: *mut u8,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) bytes: usize,
    pub(super) last_used: u64,
    pub(super) opaque: bool,
}

pub(super) struct GdiCompositingLayer {
    pub(super) content_signature: Option<u64>,
    pub(super) shadow: Option<crate::core::ShadowStyle>,
    pub(super) background: CompositingLayerBackground,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) output: GdiBitmapEntry,
    pub(super) black: Option<GdiBitmapEntry>,
    pub(super) white: Option<GdiBitmapEntry>,
    pub(super) commands: Vec<ScenePrimitive>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct GdiCompositingLayerKey {
    pub(super) scope: u64,
    pub(super) id: UiId,
}

impl GdiBitmapCache {
    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }

    pub(super) fn entry(
        &mut self,
        hdc: HDC,
        key: &str,
        width: i32,
        height: i32,
        pixels: &[u8],
    ) -> Option<&mut GdiBitmapEntry> {
        let tick = self.next_tick();
        let needs_store = self
            .entries
            .get(key)
            .is_none_or(|entry| entry.width != width || entry.height != height);
        if needs_store {
            self.misses = self.misses.saturating_add(1);
        } else {
            self.hits = self.hits.saturating_add(1);
        }
        if needs_store {
            if let Some(previous) = self.entries.remove(key) {
                self.bytes = self.bytes.saturating_sub(previous.bytes);
            }
            if pixels.len() > self.budget_bytes {
                self.publish();
                return None;
            }
            let entry = GdiBitmapEntry::new(hdc, width, height, pixels, tick)?;
            self.bytes = self.bytes.saturating_add(entry.bytes);
            self.entries.insert(key.to_string(), entry);
            self.evict_to_budget();
        }
        self.publish();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = tick;
        Some(entry)
    }

    pub(super) fn existing_entry(
        &mut self,
        key: &str,
        width: i32,
        height: i32,
    ) -> Option<&mut GdiBitmapEntry> {
        let matches = self
            .entries
            .get(key)
            .is_some_and(|entry| entry.width == width && entry.height == height);
        if !matches {
            return None;
        }

        let tick = self.next_tick();
        self.hits = self.hits.saturating_add(1);
        self.publish();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = tick;
        Some(entry)
    }

    fn evict_to_budget(&mut self) {
        while self.bytes > self.budget_bytes {
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
        self.publish();
    }

    fn trim_to(&mut self, target_bytes: usize) -> usize {
        let before = self.bytes;
        let previous = self.budget_bytes;
        self.budget_bytes = target_bytes.min(previous);
        self.evict_to_budget();
        self.budget_bytes = previous;
        before.saturating_sub(self.bytes)
    }

    fn set_budget(&mut self, budget_bytes: usize) {
        self.budget_bytes = budget_bytes;
        self.evict_to_budget();
    }

    fn publish(&self) {
        gdi_telemetry(1).publish(crate::memory::CacheUsage {
            cache_bytes: self.bytes,
            cpu_bytes: self.bytes,
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            largest_entry_bytes: self
                .entries
                .values()
                .map(|entry| entry.bytes)
                .max()
                .unwrap_or(0),
            ..Default::default()
        });
    }
}

impl GdiBitmapEntry {
    fn new(hdc: HDC, width: i32, height: i32, pixels: &[u8], last_used: u64) -> Option<Self> {
        if width <= 0 || height <= 0 || pixels.len() != (width * height * 4) as usize {
            return None;
        }
        unsafe {
            let memory_dc = CreateCompatibleDC(Some(hdc));
            if memory_dc.is_invalid() {
                return None;
            }
            let mut bits = std::ptr::null_mut();
            let bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let Ok(bitmap) =
                CreateDIBSection(Some(hdc), &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(memory_dc);
                return None;
            };
            if bitmap.is_invalid() || bits.is_null() {
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(memory_dc);
                return None;
            }
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
            let old_bitmap = SelectObject(memory_dc, bitmap.into());
            let opaque = pixels.chunks_exact(4).all(|pixel| pixel[3] == 255);
            Some(Self {
                memory_dc,
                bitmap,
                old_bitmap,
                bits: bits.cast(),
                width,
                height,
                bytes: pixels.len(),
                last_used,
                opaque,
            })
        }
    }
}

impl Drop for GdiBitmapEntry {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.memory_dc, self.old_bitmap);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.memory_dc);
        }
    }
}

impl GdiCompositingLayer {
    pub(super) fn new(
        hdc: HDC,
        width: i32,
        height: i32,
        background: CompositingLayerBackground,
    ) -> Option<Self> {
        let output_pixels = match background {
            CompositingLayerBackground::Opaque => solid_bgra_pixels(width, height, [0, 0, 0, 255])?,
            CompositingLayerBackground::Transparent => {
                solid_bgra_pixels(width, height, [0, 0, 0, 0])?
            }
        };
        let output = GdiBitmapEntry::new(hdc, width, height, &output_pixels, 0)?;
        let (black, white) = match background {
            CompositingLayerBackground::Opaque => (None, None),
            CompositingLayerBackground::Transparent => {
                let black_pixels = solid_bgra_pixels(width, height, [0, 0, 0, 255])?;
                let white_pixels = solid_bgra_pixels(width, height, [255, 255, 255, 255])?;
                (
                    Some(GdiBitmapEntry::new(hdc, width, height, &black_pixels, 0)?),
                    Some(GdiBitmapEntry::new(hdc, width, height, &white_pixels, 0)?),
                )
            }
        };
        Some(Self {
            content_signature: None,
            shadow: None,
            background,
            width,
            height,
            output,
            black,
            white,
            commands: Vec::new(),
        })
    }

    pub(super) fn redraw(&mut self, commands: &[ScenePrimitive], damage: &[UiRect]) {
        unsafe {
            let _ = GdiFlush();
        }
        match self.background {
            CompositingLayerBackground::Opaque => {
                for rect in damage {
                    fill_gdi_surface_region(&mut self.output, *rect, [0, 0, 0, 255]);
                    draw_gdi_commands_clipped(self.output.memory_dc, commands, *rect);
                }
                unsafe {
                    let _ = GdiFlush();
                }
                for rect in damage {
                    set_gdi_surface_alpha_region(&mut self.output, *rect, 255);
                }
                self.output.opaque = true;
            }
            CompositingLayerBackground::Transparent => {
                let (Some(black), Some(white)) = (&mut self.black, &mut self.white) else {
                    return;
                };
                for rect in damage {
                    fill_gdi_surface_region(black, *rect, [0, 0, 0, 255]);
                    fill_gdi_surface_region(white, *rect, [255, 255, 255, 255]);
                    draw_gdi_commands_clipped(black.memory_dc, commands, *rect);
                    draw_gdi_commands_clipped(white.memory_dc, commands, *rect);
                }
                unsafe {
                    let _ = GdiFlush();
                }
                for rect in damage {
                    synthesize_transparent_region(&mut self.output, black, white, *rect);
                }
                self.output.opaque = false;
            }
        }
    }
}

pub(super) fn solid_bgra_pixels(width: i32, height: i32, color: [u8; 4]) -> Option<Vec<u8>> {
    let len = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    let mut pixels = vec![0; len];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
    Some(pixels)
}

pub(super) fn clipped_surface_region(
    surface: &GdiBitmapEntry,
    rect: UiRect,
) -> Option<PhysicalRect> {
    pixel_rect_outward(rect).intersect(PhysicalRect::new(0, 0, surface.width, surface.height))
}

pub(super) fn fill_gdi_surface_region(surface: &mut GdiBitmapEntry, rect: UiRect, color: [u8; 4]) {
    let Some(rect) = clipped_surface_region(surface, rect) else {
        return;
    };
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let offset = ((y * surface.width + x) * 4) as usize;
            unsafe {
                std::ptr::copy_nonoverlapping(color.as_ptr(), surface.bits.add(offset), 4);
            }
        }
    }
}

pub(super) fn set_gdi_surface_alpha_region(surface: &mut GdiBitmapEntry, rect: UiRect, alpha: u8) {
    let Some(rect) = clipped_surface_region(surface, rect) else {
        return;
    };
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let offset = ((y * surface.width + x) * 4 + 3) as usize;
            unsafe {
                *surface.bits.add(offset) = alpha;
            }
        }
    }
}

pub(super) fn synthesize_transparent_region(
    output: &mut GdiBitmapEntry,
    black: &GdiBitmapEntry,
    white: &GdiBitmapEntry,
    rect: UiRect,
) {
    let Some(rect) = clipped_surface_region(output, rect) else {
        return;
    };
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let offset = ((y * output.width + x) * 4) as usize;
            unsafe {
                let black_pixel = std::slice::from_raw_parts(black.bits.add(offset), 4);
                let white_pixel = std::slice::from_raw_parts(white.bits.add(offset), 4);
                let synthesized = synthesize_transparent_pixel(black_pixel, white_pixel);
                let output_pixel = std::slice::from_raw_parts_mut(output.bits.add(offset), 4);
                output_pixel.copy_from_slice(&synthesized);
            }
        }
    }
}

pub(super) fn synthesize_transparent_pixel(black: &[u8], white: &[u8]) -> [u8; 4] {
    let backdrop = (0..3)
        .map(|channel| white[channel].saturating_sub(black[channel]) as u16)
        .sum::<u16>();
    let alpha = 255_u8.saturating_sub(((backdrop + 1) / 3) as u8);
    [
        black[0].min(alpha),
        black[1].min(alpha),
        black[2].min(alpha),
        alpha,
    ]
}

pub(super) fn draw_gdi_commands_clipped(hdc: HDC, commands: &[ScenePrimitive], clip: UiRect) {
    let saved = unsafe { SaveDC(hdc) };
    if saved == 0 {
        return;
    }
    unsafe {
        let clip_rect = win_rect(clip);
        let _ = windows::Win32::Graphics::Gdi::IntersectClipRect(
            hdc,
            clip_rect.left,
            clip_rect.top,
            clip_rect.right,
            clip_rect.bottom,
        );
    }
    let clip_region = ClipRegion::new(clip);
    for command in commands {
        if clip_region.intersects(command) {
            GdiRenderer::draw_command_clipped(hdc, command, Some(clip));
        }
    }
    unsafe {
        let _ = RestoreDC(hdc, saved);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct OverlayCacheKey {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) style_signature: u64,
}
