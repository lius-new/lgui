thread_local! {
    static OVERLAY_CACHE: RefCell<HashMap<OverlayCacheKey, Vec<u8>>> = RefCell::new(HashMap::new());
    static GDI_BITMAP_CACHE: RefCell<GdiBitmapCache> = RefCell::new(GdiBitmapCache::default());
    static GDI_COMPOSITING_LAYER_SCOPE: Cell<u64> = const { Cell::new(0) };
    static GDI_COMPOSITING_LAYERS: RefCell<HashMap<GdiCompositingLayerKey, GdiCompositingLayer>> = RefCell::new(HashMap::new());
    static GDI_FRAME_BLIT_METRICS: RefCell<GdiFrameBlitMetrics> = RefCell::new(GdiFrameBlitMetrics::default());
    static GDI_FONT_FAMILY_CACHE: RefCell<HashMap<(char, i32, i32), usize>> = RefCell::new(HashMap::new());
}

fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

fn round_coord(value: f32) -> i32 {
    value.round() as i32
}

fn pixel_rect_outward(rect: UiRect) -> PhysicalRect {
    PhysicalRect::new(
        rect.left.floor() as i32,
        rect.top.floor() as i32,
        rect.right.ceil() as i32,
        rect.bottom.ceil() as i32,
    )
}

pub fn clear_gdi_renderer_caches() {
    OVERLAY_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
    GDI_BITMAP_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
    GDI_COMPOSITING_LAYERS.with(|cache| {
        cache.borrow_mut().clear();
    });
    GDI_FONT_FAMILY_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

pub fn release_gdi_compositing_layer_scope(scope: u64) {
    GDI_COMPOSITING_LAYERS.with(|layers| {
        layers.borrow_mut().retain(|key, _| key.scope != scope);
    });
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

fn record_gdi_frame_blit(source: GdiFrameBlitSource, kind: GdiFrameBlitKind, rect: UiRect) {
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

fn source_metrics_mut(
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
enum GdiFrameBlitKind {
    BitBlt,
    AlphaBlend,
    FallbackAlphaBlend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GdiFrameBlitSource {
    StaticLayer,
    Overlay,
    Backdrop,
    Custom,
    Other,
}

const GDI_BITMAP_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct GdiBitmapCache {
    entries: HashMap<String, GdiBitmapEntry>,
    bytes: usize,
    tick: u64,
}

struct GdiBitmapEntry {
    memory_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
    bytes: usize,
    last_used: u64,
    opaque: bool,
}

struct GdiCompositingLayer {
    content_signature: Option<u64>,
    background: CompositingLayerBackground,
    width: i32,
    height: i32,
    output: GdiBitmapEntry,
    black: Option<GdiBitmapEntry>,
    white: Option<GdiBitmapEntry>,
    commands: Vec<ScenePrimitive>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GdiCompositingLayerKey {
    scope: u64,
    id: UiId,
}

impl GdiBitmapCache {
    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.tick = 0;
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }

    fn entry(
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
            if let Some(previous) = self.entries.remove(key) {
                self.bytes = self.bytes.saturating_sub(previous.bytes);
            }
            let entry = GdiBitmapEntry::new(hdc, width, height, pixels, tick)?;
            self.bytes = self.bytes.saturating_add(entry.bytes);
            self.entries.insert(key.to_string(), entry);
            self.evict_to_budget();
        }
        let entry = self.entries.get_mut(key)?;
        entry.last_used = tick;
        Some(entry)
    }

    fn evict_to_budget(&mut self) {
        while self.bytes > GDI_BITMAP_CACHE_BUDGET_BYTES && self.entries.len() > 1 {
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
            }
        }
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
    fn new(
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
            background,
            width,
            height,
            output,
            black,
            white,
            commands: Vec::new(),
        })
    }

    fn redraw(&mut self, commands: &[ScenePrimitive], damage: &[UiRect]) {
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

fn solid_bgra_pixels(width: i32, height: i32, color: [u8; 4]) -> Option<Vec<u8>> {
    let len = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    let mut pixels = vec![0; len];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
    Some(pixels)
}

fn clipped_surface_region(surface: &GdiBitmapEntry, rect: UiRect) -> Option<PhysicalRect> {
    pixel_rect_outward(rect).intersect(PhysicalRect::new(0, 0, surface.width, surface.height))
}

fn fill_gdi_surface_region(surface: &mut GdiBitmapEntry, rect: UiRect, color: [u8; 4]) {
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

fn set_gdi_surface_alpha_region(surface: &mut GdiBitmapEntry, rect: UiRect, alpha: u8) {
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

fn synthesize_transparent_region(
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

fn synthesize_transparent_pixel(black: &[u8], white: &[u8]) -> [u8; 4] {
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

fn draw_gdi_commands_clipped(hdc: HDC, commands: &[ScenePrimitive], clip: UiRect) {
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
struct OverlayCacheKey {
    width: i32,
    height: i32,
    style_signature: u64,
}
