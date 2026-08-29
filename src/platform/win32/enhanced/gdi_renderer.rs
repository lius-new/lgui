// GDI renderer backend.
//
// This file should only execute render commands with backend primitives. It must not encode
// caller-specific animation semantics or introduce long-lived caches for custom paint output:
// custom paint may represent a single animation frame.
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{POINT, RECT, SIZE},
        Graphics::{
            Gdi::{
                AlphaBlend, BitBlt, CombineRgn, CreateCompatibleDC, CreateDIBSection, CreateFontW,
                CreatePen, CreatePolygonRgn, CreateRectRgn, CreateSolidBrush, DeleteDC,
                DeleteObject, DrawTextW, Ellipse, FillRect, GdiFlush, GetGlyphIndicesW,
                GetTextExtentPoint32W, LineTo, MoveToEx, RestoreDC, RoundRect, SaveDC,
                SelectClipRgn, SelectObject, SetBkMode, SetTextCharacterExtra, SetTextColor,
                AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
                CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
                DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE,
                GGI_MARK_NONEXISTING_GLYPHS, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ,
                OUT_TT_ONLY_PRECIS, PS_SOLID, RGN_AND, SRCCOPY, TRANSPARENT, WINDING,
            },
            GdiPlus::{
                ColorAdjustTypeBitmap, ColorMatrix, ColorMatrixFlagsDefault, FillModeAlternate,
                GdipAddPathArcI, GdipAddPathBezierI, GdipAddPathLineI, GdipClosePathFigure,
                GdipCreateBitmapFromScan0, GdipCreateFromHDC, GdipCreateImageAttributes,
                GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush,
                GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDisposeImage,
                GdipDisposeImageAttributes, GdipDrawEllipseI, GdipDrawImagePointsRect,
                GdipDrawPath, GdipFillEllipseI, GdipFillPath, GdipSetImageAttributesColorMatrix,
                GdipSetInterpolationMode, GdipSetPixelOffsetMode, GdipSetSmoothingMode, GpBitmap,
                GpBrush, GpGraphics, GpImageAttributes, GpPath, GpPen,
                InterpolationModeHighQualityBicubic, Ok as GpOk, PixelOffsetModeHalf, PointF,
                SmoothingModeAntiAlias, UnitPixel,
            },
        },
    },
};

use std::{
    cell::{Cell, RefCell},
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

use lgui::platform::win32::render_trace as trace;

use super::{
    blur::with_backdrop_blur_bgra,
    custom_paint::custom_paint_bgra,
    image,
    static_layer::{self, StaticLayerDrawBackend},
};
use lgui::core::{
    compositing_layer_damage, Color, CompositingLayerBackground, CompositingLayerSpec,
    CustomPaintStyle, LayerTransform, OverlayStyle, PathStyle, Point, RadialGradientLayer, Scene,
    ScenePrimitive, Stroke, TextAlign, TextStyle, UiId, UiPath, UiPathCommand, UiRect,
    VerticalGradientLayer, VisualStyle,
};
use lgui::platform::win32::{draw_svg_icon, ui_font_family_at, ui_font_family_count};
use lgui::renderer::ClipRegion;

thread_local! {
    static OVERLAY_CACHE: RefCell<HashMap<OverlayCacheKey, Vec<u8>>> = RefCell::new(HashMap::new());
    static GDI_BITMAP_CACHE: RefCell<GdiBitmapCache> = RefCell::new(GdiBitmapCache::default());
    static GDI_COMPOSITING_LAYER_SCOPE: Cell<u64> = const { Cell::new(0) };
    static GDI_COMPOSITING_LAYERS: RefCell<HashMap<GdiCompositingLayerKey, GdiCompositingLayer>> = RefCell::new(HashMap::new());
    static GDI_FRAME_BLIT_METRICS: RefCell<GdiFrameBlitMetrics> = RefCell::new(GdiFrameBlitMetrics::default());
    static GDI_FONT_FAMILY_CACHE: RefCell<HashMap<(char, i32, i32), usize>> = RefCell::new(HashMap::new());
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
    let pixels = (rect.width().max(0) as u64).saturating_mul(rect.height().max(0) as u64);
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

fn clipped_surface_region(surface: &GdiBitmapEntry, rect: UiRect) -> Option<UiRect> {
    rect.intersect(UiRect::new(0, 0, surface.width, surface.height))
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
        let _ = windows::Win32::Graphics::Gdi::IntersectClipRect(
            hdc,
            clip.left,
            clip.top,
            clip.right,
            clip.bottom,
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

pub struct GdiRenderer;

impl GdiRenderer {
    pub fn draw_scene(hdc: HDC, list: &Scene) {
        Self::draw_scene_clipped(hdc, list, None);
    }

    pub fn draw_scene_clipped(hdc: HDC, list: &Scene, clip: Option<UiRect>) {
        Self::draw_scene_clipped_scoped(hdc, list, clip, 0);
    }

    pub fn draw_scene_clipped_scoped(hdc: HDC, list: &Scene, clip: Option<UiRect>, scope: u64) {
        GDI_COMPOSITING_LAYER_SCOPE.with(|current| current.set(scope));
        let mut layer_ids = std::collections::HashSet::new();
        collect_compositing_layer_ids(list.commands(), &mut layer_ids);
        GDI_COMPOSITING_LAYERS.with(|layers| {
            layers
                .borrow_mut()
                .retain(|key, _| key.scope != scope || layer_ids.contains(&key.id));
        });
        let _clip_guard = ClipGuard::new(hdc, clip);
        let clip_region = clip.map(ClipRegion::new);
        for command in list.commands() {
            if clip_region.is_some_and(|clip| !clip.intersects(command)) {
                continue;
            }
            Self::draw_command_clipped(hdc, command, clip);
        }
    }

    pub fn draw_command(hdc: HDC, command: &ScenePrimitive) {
        Self::draw_command_clipped(hdc, command, None);
    }

    fn draw_command_clipped(hdc: HDC, command: &ScenePrimitive, clip: Option<UiRect>) {
        match command {
            ScenePrimitive::Rect { rect, style, .. } => draw_rect(hdc, *rect, *style),
            ScenePrimitive::Ellipse { rect, style, .. } => draw_ellipse(hdc, *rect, *style),
            ScenePrimitive::Text {
                rect, text, style, ..
            } => draw_text(hdc, *rect, text, *style),
            ScenePrimitive::Line {
                start, end, stroke, ..
            } => draw_line(hdc, *start, *end, *stroke),
            ScenePrimitive::Path { path, style, .. } => draw_path(hdc, path, *style),
            ScenePrimitive::Icon {
                rect, key, style, ..
            } => draw_svg_icon(hdc, key, *rect, *style),
            ScenePrimitive::Image {
                rect, source, fit, ..
            } => image::draw_ui_image(hdc, *rect, source, *fit),
            ScenePrimitive::Overlay { rect, style, .. } => draw_overlay(hdc, *rect, style),
            ScenePrimitive::CompositingLayer {
                id,
                rect,
                spec,
                commands,
                content_signature,
                ..
            } => draw_gdi_compositing_layer(
                hdc,
                id,
                *rect,
                clip,
                *spec,
                commands,
                *content_signature,
            ),
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                draw_backdrop_blur(hdc, *rect, *style, clip)
            }
            ScenePrimitive::BackdropBlurPath {
                rect, path, style, ..
            } => draw_backdrop_blur_path(hdc, *rect, path, *style, clip),
            ScenePrimitive::Custom {
                rect, key, style, ..
            } => draw_custom(hdc, *rect, key, *style),
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                if let Some(clip) = clip {
                    if static_layer::draw_static_layer_region::<GdiStaticLayerBackend>(
                        hdc,
                        id,
                        *rect,
                        clip,
                        spec,
                        commands,
                        *child_signature,
                    ) {
                        return;
                    }
                }
                static_layer::draw_static_layer::<GdiStaticLayerBackend>(
                    hdc,
                    id,
                    *rect,
                    spec,
                    commands,
                    *child_signature,
                )
            }
            ScenePrimitive::ScrollRaster {
                id,
                viewport,
                spec,
                commands,
                child_signature,
                ..
            } => static_layer::draw_scroll_raster::<GdiStaticLayerBackend>(
                hdc,
                id,
                *viewport,
                spec,
                commands,
                *child_signature,
            ),
            ScenePrimitive::Clip { rect, commands, .. } => {
                let nested_clip = match clip {
                    Some(clip) => {
                        let Some(intersection) = clip.intersect(*rect) else {
                            return;
                        };
                        Some(intersection)
                    }
                    None => Some(*rect),
                };
                let _clip_guard = ClipGuard::new(hdc, nested_clip);
                let clip_region = nested_clip.map(ClipRegion::new);
                for command in commands {
                    if clip_region.is_some_and(|clip| !clip.intersects(command)) {
                        continue;
                    }
                    Self::draw_command_clipped(hdc, command, nested_clip);
                }
            }
            ScenePrimitive::ClipPath {
                rect,
                path,
                commands,
                ..
            } => {
                let nested_clip = match clip {
                    Some(clip) => {
                        let Some(intersection) = clip.intersect(*rect) else {
                            return;
                        };
                        Some(intersection)
                    }
                    None => Some(*rect),
                };
                let Some(_clip_guard) = PolygonClipGuard::new(hdc, path, nested_clip) else {
                    return;
                };
                let clip_region = nested_clip.map(ClipRegion::new);
                for command in commands {
                    if clip_region.is_some_and(|clip| !clip.intersects(command)) {
                        continue;
                    }
                    Self::draw_command_clipped(hdc, command, nested_clip);
                }
            }
            ScenePrimitive::Glow { .. } => {}
        }
    }
}

fn collect_compositing_layer_ids(
    commands: &[ScenePrimitive],
    ids: &mut std::collections::HashSet<UiId>,
) {
    for command in commands {
        match command {
            ScenePrimitive::CompositingLayer { id, commands, .. } => {
                ids.insert(id.clone());
                collect_compositing_layer_ids(commands, ids);
            }
            ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_compositing_layer_ids(commands, ids);
            }
            _ => {}
        }
    }
}

fn draw_gdi_compositing_layer(
    hdc: HDC,
    id: &UiId,
    rect: UiRect,
    clip: Option<UiRect>,
    spec: CompositingLayerSpec,
    commands: &[ScenePrimitive],
    content_signature: u64,
) {
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let key = GdiCompositingLayerKey {
        scope: GDI_COMPOSITING_LAYER_SCOPE.with(Cell::get),
        id: id.clone(),
    };
    let previous = GDI_COMPOSITING_LAYERS.with(|layers| layers.borrow_mut().remove(&key));
    let mut layer = match previous {
        Some(layer)
            if layer.width == width
                && layer.height == height
                && layer.background == spec.background =>
        {
            layer
        }
        _ => {
            let Some(layer) = GdiCompositingLayer::new(hdc, width, height, spec.background) else {
                draw_gdi_compositing_layer_fallback(hdc, rect, clip, commands);
                return;
            };
            layer
        }
    };

    if layer.content_signature != Some(content_signature) {
        let bounds = UiRect::new(0, 0, width, height);
        let damage = if layer.commands.is_empty() {
            vec![bounds]
        } else {
            compositing_layer_damage(&layer.commands, commands, bounds)
        };
        layer.redraw(commands, &damage);
        layer.content_signature = Some(content_signature);
        layer.commands = commands.to_vec();
    }

    if spec.opacity == 0 {
        GDI_COMPOSITING_LAYERS.with(|layers| {
            layers.borrow_mut().insert(key, layer);
        });
        return;
    }

    if !spec.transform.is_identity() {
        let bounds = spec.transform.transformed_bounds(rect);
        if clip.and_then(|clip| clip.intersect(bounds)).is_none() && clip.is_some() {
            GDI_COMPOSITING_LAYERS.with(|layers| {
                layers.borrow_mut().insert(key, layer);
            });
            return;
        }
        record_gdi_frame_blit(
            GdiFrameBlitSource::StaticLayer,
            GdiFrameBlitKind::AlphaBlend,
            bounds,
        );
        draw_gdi_transformed_bitmap(hdc, rect, clip, &layer.output, spec.opacity, spec.transform);
        GDI_COMPOSITING_LAYERS.with(|layers| {
            layers.borrow_mut().insert(key, layer);
        });
        return;
    }

    let dest = match clip {
        Some(clip) => {
            let Some(dest) = rect.intersect(clip) else {
                GDI_COMPOSITING_LAYERS.with(|layers| {
                    layers.borrow_mut().insert(key, layer);
                });
                return;
            };
            dest
        }
        None => rect,
    };
    let source = UiRect::new(
        dest.left - rect.left,
        dest.top - rect.top,
        dest.right - rect.left,
        dest.bottom - rect.top,
    );
    unsafe {
        if spec.opacity == 255 && layer.output.opaque {
            record_gdi_frame_blit(
                GdiFrameBlitSource::StaticLayer,
                GdiFrameBlitKind::BitBlt,
                dest,
            );
            let _ = BitBlt(
                hdc,
                dest.left,
                dest.top,
                dest.width(),
                dest.height(),
                Some(layer.output.memory_dc),
                source.left,
                source.top,
                SRCCOPY,
            );
        } else {
            record_gdi_frame_blit(
                GdiFrameBlitSource::StaticLayer,
                GdiFrameBlitKind::AlphaBlend,
                dest,
            );
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: spec.opacity,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = AlphaBlend(
                hdc,
                dest.left,
                dest.top,
                dest.width(),
                dest.height(),
                layer.output.memory_dc,
                source.left,
                source.top,
                source.width(),
                source.height(),
                blend,
            );
        }
    }
    GDI_COMPOSITING_LAYERS.with(|layers| {
        layers.borrow_mut().insert(key, layer);
    });
}

const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;

fn draw_gdi_transformed_bitmap(
    hdc: HDC,
    rect: UiRect,
    clip: Option<UiRect>,
    bitmap: &GdiBitmapEntry,
    opacity: u8,
    transform: LayerTransform,
) {
    let _clip = ClipGuard::new(hdc, clip);
    let points = gdi_layer_destination_points(rect, transform);
    unsafe {
        let mut image: *mut GpBitmap = std::ptr::null_mut();
        if GdipCreateBitmapFromScan0(
            bitmap.width,
            bitmap.height,
            bitmap.width * 4,
            PIXEL_FORMAT_32BPP_PARGB,
            Some(bitmap.bits.cast_const()),
            &mut image,
        ) != GpOk
            || image.is_null()
        {
            return;
        }

        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            let _ = GdipDisposeImage(image.cast());
            return;
        }
        let _ = GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let mut attributes: *mut GpImageAttributes = std::ptr::null_mut();
        let attributes_ptr = if opacity == 255 {
            std::ptr::null()
        } else if GdipCreateImageAttributes(&mut attributes) == GpOk && !attributes.is_null() {
            let alpha = opacity as f32 / 255.0;
            let matrix = ColorMatrix {
                m: [
                    1.0, 0.0, 0.0, 0.0, 0.0, // red
                    0.0, 1.0, 0.0, 0.0, 0.0, // green
                    0.0, 0.0, 1.0, 0.0, 0.0, // blue
                    0.0, 0.0, 0.0, alpha, 0.0, // alpha
                    0.0, 0.0, 0.0, 0.0, 1.0,
                ],
            };
            let _ = GdipSetImageAttributesColorMatrix(
                attributes,
                ColorAdjustTypeBitmap,
                true,
                &matrix,
                std::ptr::null(),
                ColorMatrixFlagsDefault,
            );
            attributes.cast_const()
        } else {
            std::ptr::null()
        };

        let _ = GdipDrawImagePointsRect(
            graphics,
            image.cast(),
            points.as_ptr(),
            points.len() as i32,
            0.0,
            0.0,
            bitmap.width as f32,
            bitmap.height as f32,
            UnitPixel,
            attributes_ptr,
            0,
            std::ptr::null_mut(),
        );

        if !attributes.is_null() {
            let _ = GdipDisposeImageAttributes(attributes);
        }
        let _ = GdipDeleteGraphics(graphics);
        let _ = GdipDisposeImage(image.cast());
    }
}

fn gdi_layer_destination_points(rect: UiRect, transform: LayerTransform) -> [PointF; 3] {
    let transform_point = |x, y| {
        let (x, y) = transform.transform_point(rect, x, y);
        PointF { X: x, Y: y }
    };
    [
        transform_point(rect.left as f32, rect.top as f32),
        transform_point(rect.right as f32, rect.top as f32),
        transform_point(rect.left as f32, rect.bottom as f32),
    ]
}

fn draw_gdi_compositing_layer_fallback(
    hdc: HDC,
    rect: UiRect,
    clip: Option<UiRect>,
    commands: &[ScenePrimitive],
) {
    let saved = unsafe { SaveDC(hdc) };
    if saved == 0 {
        return;
    }
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::SetViewportOrgEx(hdc, rect.left, rect.top, None);
    }
    let local_clip = clip
        .and_then(|clip| rect.intersect(clip))
        .map(|clip| clip.translate(-rect.left, -rect.top));
    for command in commands {
        if local_clip.is_none_or(|clip| ClipRegion::new(clip).intersects(command)) {
            GdiRenderer::draw_command_clipped(hdc, command, local_clip);
        }
    }
    unsafe {
        let _ = RestoreDC(hdc, saved);
    }
}

fn draw_backdrop_blur(
    hdc: HDC,
    rect: UiRect,
    style: lgui::core::BackdropBlurStyle,
    clip: Option<UiRect>,
) {
    let _ = with_backdrop_blur_bgra(rect, style, |pixels, width, height, opacity| {
        let source_alpha = (opacity * 255.0).round() as u8;
        if source_alpha == 0 {
            return;
        }
        if let Some(clip) = clip {
            let Some(dest) = rect.intersect(clip) else {
                return;
            };
            let source = UiRect::new(
                dest.left - rect.left,
                dest.top - rect.top,
                dest.right - rect.left,
                dest.bottom - rect.top,
            );
            let key = backdrop_gdi_cache_key(rect, style);
            if blit_cached_gdi_bitmap(
                GdiFrameBlitSource::Backdrop,
                hdc,
                &key,
                dest,
                source,
                width,
                height,
                pixels,
                source_alpha,
            ) {
                return;
            }
            blit_premultiplied_bgra_region_alpha_with_source(
                GdiFrameBlitSource::Backdrop,
                hdc,
                dest,
                source,
                width,
                height,
                pixels,
                source_alpha,
            );
        } else {
            let key = backdrop_gdi_cache_key(rect, style);
            if blit_cached_gdi_bitmap(
                GdiFrameBlitSource::Backdrop,
                hdc,
                &key,
                rect,
                UiRect::new(0, 0, width, height),
                width,
                height,
                pixels,
                source_alpha,
            ) {
                return;
            }
            blit_premultiplied_bgra_alpha_with_source(
                GdiFrameBlitSource::Backdrop,
                hdc,
                rect,
                width,
                height,
                pixels,
                source_alpha,
            );
        }
    });
}

fn draw_backdrop_blur_path(
    hdc: HDC,
    rect: UiRect,
    path: &UiPath,
    style: lgui::core::BackdropBlurStyle,
    clip: Option<UiRect>,
) {
    let _ = with_backdrop_blur_bgra(rect, style, |pixels, width, height, opacity| {
        let source_alpha = (opacity * 255.0).round() as u8;
        if source_alpha == 0 {
            return;
        }
        let Some(_clip_guard) = PolygonClipGuard::new(hdc, path, clip) else {
            if let Some(clip) = clip {
                let Some(dest) = rect.intersect(clip) else {
                    return;
                };
                let source = UiRect::new(
                    dest.left - rect.left,
                    dest.top - rect.top,
                    dest.right - rect.left,
                    dest.bottom - rect.top,
                );
                let key = backdrop_gdi_cache_key(rect, style);
                if blit_cached_gdi_bitmap(
                    GdiFrameBlitSource::Backdrop,
                    hdc,
                    &key,
                    dest,
                    source,
                    width,
                    height,
                    pixels,
                    source_alpha,
                ) {
                    return;
                }
                blit_premultiplied_bgra_region_alpha_with_source(
                    GdiFrameBlitSource::Backdrop,
                    hdc,
                    dest,
                    source,
                    width,
                    height,
                    pixels,
                    source_alpha,
                );
            } else {
                let key = backdrop_gdi_cache_key(rect, style);
                if blit_cached_gdi_bitmap(
                    GdiFrameBlitSource::Backdrop,
                    hdc,
                    &key,
                    rect,
                    UiRect::new(0, 0, width, height),
                    width,
                    height,
                    pixels,
                    source_alpha,
                ) {
                    return;
                }
                blit_premultiplied_bgra_alpha_with_source(
                    GdiFrameBlitSource::Backdrop,
                    hdc,
                    rect,
                    width,
                    height,
                    pixels,
                    source_alpha,
                );
            }
            return;
        };
        let key = backdrop_gdi_cache_key(rect, style);
        if blit_cached_gdi_bitmap(
            GdiFrameBlitSource::Backdrop,
            hdc,
            &key,
            rect,
            UiRect::new(0, 0, width, height),
            width,
            height,
            pixels,
            source_alpha,
        ) {
            return;
        }
        blit_premultiplied_bgra_alpha_with_source(
            GdiFrameBlitSource::Backdrop,
            hdc,
            rect,
            width,
            height,
            pixels,
            source_alpha,
        );
    });
}

fn backdrop_gdi_cache_key(rect: UiRect, style: lgui::core::BackdropBlurStyle) -> String {
    let mut hasher = DefaultHasher::new();
    "backdrop-blur-gdi".hash(&mut hasher);
    style.source.hash(&mut hasher);
    style.fit.hash(&mut hasher);
    rect.left.hash(&mut hasher);
    rect.top.hash(&mut hasher);
    rect.right.hash(&mut hasher);
    rect.bottom.hash(&mut hasher);
    style.source_rect.left.hash(&mut hasher);
    style.source_rect.top.hash(&mut hasher);
    style.source_rect.right.hash(&mut hasher);
    style.source_rect.bottom.hash(&mut hasher);
    style.radius.hash(&mut hasher);
    style.tint.0.hash(&mut hasher);
    style.tint_alpha.to_bits().hash(&mut hasher);
    format!("backdrop:{:016x}", hasher.finish())
}

struct ClipGuard {
    hdc: HDC,
    state: Option<i32>,
}

impl ClipGuard {
    fn new(hdc: HDC, clip: Option<UiRect>) -> Self {
        let Some(clip) = clip else {
            return Self { hdc, state: None };
        };
        unsafe {
            let state = SaveDC(hdc);
            if state == 0 {
                return Self { hdc, state: None };
            }
            let region = CreateRectRgn(clip.left, clip.top, clip.right, clip.bottom);
            if region.is_invalid() {
                let _ = RestoreDC(hdc, state);
                return Self { hdc, state: None };
            }
            let _ = SelectClipRgn(hdc, Some(region));
            let _ = DeleteObject(region.into());
            Self {
                hdc,
                state: Some(state),
            }
        }
    }
}

impl Drop for ClipGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state {
            unsafe {
                let _ = RestoreDC(self.hdc, state);
            }
        }
    }
}

struct PolygonClipGuard {
    hdc: HDC,
    state: i32,
}

impl PolygonClipGuard {
    fn new(hdc: HDC, path: &UiPath, clip: Option<UiRect>) -> Option<Self> {
        let points = polygon_points(path)?;
        if points.len() < 3 {
            return None;
        }

        unsafe {
            let state = SaveDC(hdc);
            if state == 0 {
                return None;
            }

            let polygon_region = CreatePolygonRgn(&points, WINDING);
            if polygon_region.is_invalid() {
                let _ = RestoreDC(hdc, state);
                return None;
            }

            if let Some(clip) = clip {
                let clip_region = CreateRectRgn(clip.left, clip.top, clip.right, clip.bottom);
                if clip_region.is_invalid() {
                    let _ = DeleteObject(polygon_region.into());
                    let _ = RestoreDC(hdc, state);
                    return None;
                }
                let _ = CombineRgn(
                    Some(polygon_region),
                    Some(polygon_region),
                    Some(clip_region),
                    RGN_AND,
                );
                let _ = DeleteObject(clip_region.into());
            }

            let _ = SelectClipRgn(hdc, Some(polygon_region));
            let _ = DeleteObject(polygon_region.into());
            Some(Self { hdc, state })
        }
    }
}

impl Drop for PolygonClipGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = RestoreDC(self.hdc, self.state);
        }
    }
}

fn polygon_points(path: &UiPath) -> Option<Vec<POINT>> {
    let mut points = Vec::new();
    let mut has_close = false;
    for command in path.commands() {
        match *command {
            UiPathCommand::MoveTo(point) | UiPathCommand::LineTo(point) => {
                points.push(POINT {
                    x: point.x,
                    y: point.y,
                });
            }
            UiPathCommand::Close => {
                has_close = true;
            }
            UiPathCommand::QuadraticTo { .. } | UiPathCommand::CubicTo { .. } => return None,
        }
    }
    if has_close {
        Some(points)
    } else {
        None
    }
}

struct GdiStaticLayerBackend;

impl StaticLayerDrawBackend for GdiStaticLayerBackend {
    fn draw_command(hdc: HDC, command: &ScenePrimitive) {
        GdiRenderer::draw_command(hdc, command);
    }

    fn blit_premultiplied_bgra(hdc: HDC, rect: UiRect, width: i32, height: i32, pixels: &[u8]) {
        blit_premultiplied_bgra_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            rect,
            width,
            height,
            pixels,
        );
    }

    fn blit_premultiplied_bgra_alpha(
        hdc: HDC,
        rect: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    ) {
        blit_premultiplied_bgra_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            rect,
            width,
            height,
            pixels,
            source_alpha,
        );
    }

    fn blit_premultiplied_bgra_region(
        hdc: HDC,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
    ) {
        blit_premultiplied_bgra_region_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            dest,
            source,
            width,
            height,
            pixels,
            255,
        );
    }

    fn blit_premultiplied_bgra_region_alpha(
        hdc: HDC,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    ) {
        blit_premultiplied_bgra_region_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        );
    }

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
        if blit_cached_gdi_bitmap(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            cache_key,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        ) {
            return;
        }
        blit_premultiplied_bgra_region_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
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
    ) -> Option<T> {
        with_dib_section(hdc, width, height, draw)
    }

    fn clear_alpha_buffer(bits: *mut std::ffi::c_void, width: i32, height: i32) {
        clear_alpha_buffer(bits, width, height);
    }

    fn prepare_alpha_buffer(
        bits: *mut std::ffi::c_void,
        width: i32,
        height: i32,
        background: lgui::core::StaticLayerBackground,
    ) {
        prepare_alpha_buffer(bits, width, height, background);
    }
}

fn draw_line(hdc: HDC, start: Point, end: Point, stroke: Stroke) {
    if stroke.alpha == 0 || stroke.width <= 0 {
        return;
    }
    unsafe {
        let pen = CreatePen(PS_SOLID, stroke.width, colorref(stroke.color));
        let old_pen = SelectObject(hdc, pen.into());
        let _ = MoveToEx(hdc, start.x, start.y, None);
        let _ = LineTo(hdc, end.x, end.y);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
    }
}

fn draw_rect(hdc: HDC, rect: UiRect, style: VisualStyle) {
    if style.fill_alpha == 0 && style.stroke.map(|stroke| stroke.alpha).unwrap_or(0) == 0 {
        return;
    }
    let rect = win_rect(rect);
    if draw_antialiased_rect(hdc, rect, style) {
        return;
    }

    unsafe {
        let brush = style
            .fill
            .map(|color| CreateSolidBrush(colorref(color)))
            .unwrap_or(HBRUSH::default());
        let pen = style
            .stroke
            .map(|stroke| CreatePen(PS_SOLID, stroke.width, colorref(stroke.color)));

        if let Some(pen) = pen {
            let old_pen = SelectObject(hdc, pen.into());
            if style.fill.is_some() {
                let old_brush = SelectObject(hdc, brush.into());
                let _ = RoundRect(
                    hdc,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    style.radius * 2,
                    style.radius * 2,
                );
                let _ = SelectObject(hdc, old_brush);
            } else {
                let _ = RoundRect(
                    hdc,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    style.radius * 2,
                    style.radius * 2,
                );
            }
            let _ = SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen.into());
        } else if style.fill.is_some() {
            let _ = FillRect(hdc, &rect, brush);
        }

        if style.fill.is_some() {
            let _ = DeleteObject(brush.into());
        }
    }
}

fn draw_ellipse(hdc: HDC, rect: UiRect, style: VisualStyle) {
    if style.fill_alpha == 0 && style.stroke.map(|stroke| stroke.alpha).unwrap_or(0) == 0 {
        return;
    }
    let rect = win_rect(rect);
    if draw_antialiased_ellipse(hdc, rect, style) {
        return;
    }

    unsafe {
        let fill = style.fill.unwrap_or(Color::BLACK);
        let stroke = style.stroke.unwrap_or(Stroke::new(fill, 1, 0));
        let brush = CreateSolidBrush(colorref(fill));
        let pen = CreatePen(PS_SOLID, stroke.width, colorref(stroke.color));
        let old_brush = SelectObject(hdc, brush.into());
        let old_pen = SelectObject(hdc, pen.into());
        let _ = Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

fn draw_overlay(hdc: HDC, rect: UiRect, style: &OverlayStyle) {
    let start = Instant::now();
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let key = OverlayCacheKey {
        width,
        height,
        style_signature: overlay_signature(style),
    };
    OVERLAY_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&key) {
            cache.insert(key, rasterize_overlay(width, height, style));
        }
        if let Some(pixels) = cache.get(&key) {
            let gdi_key = overlay_gdi_cache_key(&key);
            if blit_cached_gdi_bitmap(
                GdiFrameBlitSource::Overlay,
                hdc,
                &gdi_key,
                rect,
                UiRect::new(0, 0, width, height),
                width,
                height,
                pixels,
                255,
            ) {
                return;
            }
            blit_premultiplied_bgra_with_source(
                GdiFrameBlitSource::Overlay,
                hdc,
                rect,
                width,
                height,
                pixels,
            );
        }
    });
    trace_duration("gdi.draw_overlay", start.elapsed());
}

fn overlay_gdi_cache_key(key: &OverlayCacheKey) -> String {
    format!(
        "overlay:{}:{}:{:016x}",
        key.width, key.height, key.style_signature
    )
}

fn rasterize_overlay(width: i32, height: i32, style: &OverlayStyle) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for layer in &style.vertical_layers {
        composite_vertical_gradient(&mut pixels, width, height, *layer);
    }
    for layer in &style.radial_layers {
        composite_radial_gradient(&mut pixels, width, height, *layer);
    }
    pixels
}

fn overlay_signature(style: &OverlayStyle) -> u64 {
    let mut hasher = DefaultHasher::new();
    style.vertical_layers.len().hash(&mut hasher);
    for layer in &style.vertical_layers {
        layer.color.0.hash(&mut hasher);
        layer.alpha_top.to_bits().hash(&mut hasher);
        layer.alpha_bottom.to_bits().hash(&mut hasher);
    }
    style.radial_layers.len().hash(&mut hasher);
    for layer in &style.radial_layers {
        layer.color.0.hash(&mut hasher);
        layer.alpha.to_bits().hash(&mut hasher);
        layer.center_x.to_bits().hash(&mut hasher);
        layer.center_y.to_bits().hash(&mut hasher);
        layer.radius.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn draw_custom(hdc: HDC, rect: UiRect, key: &str, style: Option<CustomPaintStyle>) {
    let Some(style) = style else {
        return;
    };
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let raster_start = Instant::now();
    let Some(pixels) = custom_paint_bgra(key, width, height, style) else {
        return;
    };
    trace_custom_duration("gdi.draw_custom.raster", key, raster_start.elapsed());
    let blit_start = Instant::now();
    blit_premultiplied_bgra_with_source(
        GdiFrameBlitSource::Custom,
        hdc,
        rect,
        width,
        height,
        &pixels,
    );
    trace_custom_duration("gdi.draw_custom.blit", key, blit_start.elapsed());
}

fn composite_vertical_gradient(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    layer: VerticalGradientLayer,
) {
    let (red, green, blue) = color_components(layer.color);
    for y in 0..height {
        let t = if height <= 1 {
            1.0
        } else {
            y as f32 / (height - 1) as f32
        };
        let alpha = layer.alpha_top + (layer.alpha_bottom - layer.alpha_top) * t;
        let alpha_u8 = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha_u8 == 0 {
            continue;
        }
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            composite_premultiplied_pixel(
                &mut pixels[index..index + 4],
                red,
                green,
                blue,
                alpha_u8,
            );
        }
    }
}

fn composite_radial_gradient(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    layer: RadialGradientLayer,
) {
    let (red, green, blue) = color_components(layer.color);
    let center_x = width as f32 * layer.center_x;
    let center_y = height as f32 * layer.center_y;
    let radius = (width.min(height) as f32 * layer.radius).max(1.0);

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let distance = ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
            let falloff = (1.0 - distance).powf(2.0);
            let alpha = (layer.alpha * falloff).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let index = ((y * width + x) * 4) as usize;
            composite_premultiplied_pixel(
                &mut pixels[index..index + 4],
                red,
                green,
                blue,
                (alpha * 255.0).round() as u8,
            );
        }
    }
}

fn composite_premultiplied_pixel(pixel: &mut [u8], red: u8, green: u8, blue: u8, alpha: u8) {
    let alpha_f = alpha as f32 / 255.0;
    let inv_alpha = 1.0 - alpha_f;
    pixel[0] = ((blue as f32 * alpha_f) + (pixel[0] as f32 * inv_alpha)).round() as u8;
    pixel[1] = ((green as f32 * alpha_f) + (pixel[1] as f32 * inv_alpha)).round() as u8;
    pixel[2] = ((red as f32 * alpha_f) + (pixel[2] as f32 * inv_alpha)).round() as u8;
    pixel[3] = ((alpha as f32) + (pixel[3] as f32 * inv_alpha)).round() as u8;
}

fn blit_premultiplied_bgra(hdc: HDC, rect: UiRect, width: i32, height: i32, pixels: &[u8]) {
    blit_premultiplied_bgra_alpha(hdc, rect, width, height, pixels, 255);
}

fn blit_premultiplied_bgra_with_source(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    rect: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
) {
    blit_premultiplied_bgra_alpha_with_source(source_kind, hdc, rect, width, height, pixels, 255);
}

fn blit_premultiplied_bgra_alpha(
    hdc: HDC,
    rect: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) {
    blit_premultiplied_bgra_alpha_with_source(
        GdiFrameBlitSource::Other,
        hdc,
        rect,
        width,
        height,
        pixels,
        source_alpha,
    );
}

fn blit_premultiplied_bgra_alpha_with_source(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    rect: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) {
    blit_premultiplied_bgra_region_alpha_with_source(
        source_kind,
        hdc,
        rect,
        UiRect::new(0, 0, width, height),
        width,
        height,
        pixels,
        source_alpha,
    );
}

fn blit_premultiplied_bgra_region(
    hdc: HDC,
    dest: UiRect,
    source: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
) {
    blit_premultiplied_bgra_region_alpha(hdc, dest, source, width, height, pixels, 255);
}

fn blit_cached_gdi_bitmap(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    cache_key: &str,
    dest: UiRect,
    source: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) -> bool {
    if dest.width() <= 0 || dest.height() <= 0 || source.width() <= 0 || source.height() <= 0 {
        return true;
    }
    GDI_BITMAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let Some(entry) = cache.entry(hdc, cache_key, width, height, pixels) else {
            return false;
        };
        unsafe {
            if source_alpha == 255 && entry.opaque {
                record_gdi_frame_blit(source_kind, GdiFrameBlitKind::BitBlt, dest);
                let _ = BitBlt(
                    hdc,
                    dest.left,
                    dest.top,
                    dest.width(),
                    dest.height(),
                    Some(entry.memory_dc),
                    source.left,
                    source.top,
                    SRCCOPY,
                );
                return true;
            }
            record_gdi_frame_blit(source_kind, GdiFrameBlitKind::AlphaBlend, dest);
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: source_alpha,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = AlphaBlend(
                hdc,
                dest.left,
                dest.top,
                dest.width(),
                dest.height(),
                entry.memory_dc,
                source.left,
                source.top,
                source.width(),
                source.height(),
                blend,
            );
        }
        true
    })
}

fn blit_premultiplied_bgra_region_alpha(
    hdc: HDC,
    dest: UiRect,
    source: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) {
    blit_premultiplied_bgra_region_alpha_with_source(
        GdiFrameBlitSource::Other,
        hdc,
        dest,
        source,
        width,
        height,
        pixels,
        source_alpha,
    );
}

fn blit_premultiplied_bgra_region_alpha_with_source(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    dest: UiRect,
    source: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) {
    unsafe {
        record_gdi_frame_blit(source_kind, GdiFrameBlitKind::FallbackAlphaBlend, dest);
        let memory_dc = CreateCompatibleDC(Some(hdc));
        if memory_dc.is_invalid() {
            return;
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
            return;
        };
        if bitmap.is_invalid() || bits.is_null() {
            let _ = DeleteDC(memory_dc);
            return;
        }

        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
        let old_bitmap = SelectObject(memory_dc, bitmap.into());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: source_alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = AlphaBlend(
            hdc,
            dest.left,
            dest.top,
            dest.width(),
            dest.height(),
            memory_dc,
            source.left,
            source.top,
            source.width(),
            source.height(),
            blend,
        );
        let _ = SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_dc);
    }
}

fn with_dib_section<T>(
    hdc: HDC,
    width: i32,
    height: i32,
    draw: impl FnOnce(HDC, *mut std::ffi::c_void) -> T,
) -> Option<T> {
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

        let old_bitmap = SelectObject(memory_dc, bitmap.into());
        let result = draw(memory_dc, bits);
        let _ = SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_dc);
        Some(result)
    }
}

fn clear_alpha_buffer(bits: *mut std::ffi::c_void, width: i32, height: i32) {
    if bits.is_null() {
        return;
    }
    let len = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), len) };
    pixels.fill(0);
}

fn prepare_alpha_buffer(
    bits: *mut std::ffi::c_void,
    width: i32,
    height: i32,
    background: lgui::core::StaticLayerBackground,
) {
    match background {
        lgui::core::StaticLayerBackground::Opaque => set_opaque_alpha(bits, width, height),
        lgui::core::StaticLayerBackground::Transparent => {
            set_drawn_pixel_alpha(bits, width, height)
        }
    }
}

fn set_opaque_alpha(bits: *mut std::ffi::c_void, width: i32, height: i32) {
    if bits.is_null() {
        return;
    }
    let len = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), len) };
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

fn set_drawn_pixel_alpha(bits: *mut std::ffi::c_void, width: i32, height: i32) {
    if bits.is_null() {
        return;
    }
    let len = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), len) };
    for pixel in pixels.chunks_exact_mut(4) {
        if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
            pixel[3] = 255;
        }
    }
}

fn color_components(color: Color) -> (u8, u8, u8) {
    (
        ((color.0 >> 16) & 0xFF) as u8,
        ((color.0 >> 8) & 0xFF) as u8,
        (color.0 & 0xFF) as u8,
    )
}

fn trace_duration(label: &str, duration: Duration) {
    if trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}

fn trace_custom_duration(label: &str, key: &str, duration: Duration) {
    if trace::duration_detail_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: key={key} {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_surface_reconstruction_preserves_black_and_partial_alpha() {
        assert_eq!(
            synthesize_transparent_pixel(&[0, 0, 0, 255], &[127, 127, 127, 255]),
            [0, 0, 0, 128]
        );
        assert_eq!(
            synthesize_transparent_pixel(&[0, 0, 128, 255], &[127, 127, 255, 255]),
            [0, 0, 128, 128]
        );
    }

    #[test]
    fn transparent_surface_reconstruction_handles_clear_and_opaque_pixels() {
        assert_eq!(
            synthesize_transparent_pixel(&[0, 0, 0, 255], &[255, 255, 255, 255]),
            [0, 0, 0, 0]
        );
        assert_eq!(
            synthesize_transparent_pixel(&[10, 20, 30, 255], &[10, 20, 30, 255]),
            [10, 20, 30, 255]
        );
    }

    #[test]
    fn transformed_destination_points_follow_layer_origin() {
        let rect = UiRect::new(10, 20, 30, 60);
        let transform = LayerTransform::identity()
            .scale_xy(2.0, 1.0)
            .rotation_degrees(90.0)
            .translation(75.0, -25.0)
            .origin(0.5, 0.5);
        let points = gdi_layer_destination_points(rect, transform);
        let expected = [
            transform.transform_point(rect, 10.0, 20.0),
            transform.transform_point(rect, 30.0, 20.0),
            transform.transform_point(rect, 10.0, 60.0),
        ];
        for (point, expected) in points.iter().zip(expected) {
            assert!((point.X - expected.0).abs() < 0.001);
            assert!((point.Y - expected.1).abs() < 0.001);
        }
    }

    #[test]
    fn transparent_gdi_layer_updates_only_changed_black_content() {
        let seed = unsafe { CreateCompatibleDC(None) };
        assert!(!seed.is_invalid());
        let mut layer =
            GdiCompositingLayer::new(seed, 32, 32, CompositingLayerBackground::Transparent)
                .expect("create test layer");
        let line = |id_value: &str, y| ScenePrimitive::Line {
            id: UiId::owned(id_value.to_string()),
            start: Point::new(4, y),
            end: Point::new(24, y),
            stroke: Stroke::new(Color::BLACK, 1, 255),
            phase: lgui::core::RenderPhase::Content,
        };
        let previous = vec![line("black-line", 6)];
        layer.redraw(&previous, &[UiRect::new(0, 0, 32, 32)]);
        assert_eq!(surface_pixel(&layer.output, 12, 6), [0, 0, 0, 255]);
        assert_eq!(surface_pixel(&layer.output, 12, 20), [0, 0, 0, 0]);

        let next = vec![line("black-line", 20)];
        let damage = compositing_layer_damage(
            &previous,
            &next,
            UiRect::new(0, 0, layer.width, layer.height),
        );
        layer.redraw(&next, &damage);
        assert_eq!(surface_pixel(&layer.output, 12, 6), [0, 0, 0, 0]);
        assert_eq!(surface_pixel(&layer.output, 12, 20), [0, 0, 0, 255]);

        unsafe {
            let _ = DeleteDC(seed);
        }
    }

    fn surface_pixel(surface: &GdiBitmapEntry, x: i32, y: i32) -> [u8; 4] {
        let offset = ((y * surface.width + x) * 4) as usize;
        unsafe {
            let pixel = std::slice::from_raw_parts(surface.bits.add(offset), 4);
            [pixel[0], pixel[1], pixel[2], pixel[3]]
        }
    }
}

fn draw_text(hdc: HDC, rect: UiRect, text: &str, style: TextStyle) {
    if style.alpha == 0 || text.is_empty() {
        return;
    }
    unsafe {
        let runs = gdi_text_runs(hdc, text, style.height, style.weight);
        let previous_extra = SetTextCharacterExtra(hdc, style.tracking);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, colorref(style.color));

        if runs.len() <= 1 {
            let family_index = runs.first().map(|run| run.family_index).unwrap_or(0);
            if let Some(font) = create_gdi_font(style.height, style.weight, family_index) {
                let old_font = SelectObject(hdc, font.into());
                let align = match style.align {
                    TextAlign::Left => DT_LEFT,
                    TextAlign::Center => DT_CENTER,
                    TextAlign::Right => DT_RIGHT,
                };
                let mut draw_rect = win_rect(rect);
                let mut wide: Vec<u16> = text.encode_utf16().collect();
                let _ = DrawTextW(
                    hdc,
                    &mut wide,
                    &mut draw_rect,
                    DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | align,
                );
                let _ = SelectObject(hdc, old_font);
                let _ = DeleteObject(font.into());
            }
        } else {
            let total_width =
                gdi_text_runs_width(hdc, &runs, style.height, style.weight, style.tracking);
            let mut left = match style.align {
                TextAlign::Left => rect.left,
                TextAlign::Center => rect.left + ((rect.width() - total_width).max(0) / 2),
                TextAlign::Right => rect.right - total_width,
            };
            for run in &runs {
                if let Some(font) = create_gdi_font(style.height, style.weight, run.family_index) {
                    let old_font = SelectObject(hdc, font.into());
                    let run_width = gdi_text_width(hdc, &run.text, style.tracking).unwrap_or(0);
                    let mut run_rect = RECT {
                        left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    };
                    let mut wide: Vec<u16> = run.text.encode_utf16().collect();
                    let _ = DrawTextW(
                        hdc,
                        &mut wide,
                        &mut run_rect,
                        DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_LEFT,
                    );
                    left += run_width;
                    let _ = SelectObject(hdc, old_font);
                    let _ = DeleteObject(font.into());
                }
            }
        };

        let _ = SetTextCharacterExtra(hdc, previous_extra);
    }
}

#[derive(Clone)]
struct GdiTextRun {
    family_index: usize,
    text: String,
}

fn gdi_text_runs(hdc: HDC, text: &str, height: i32, weight: i32) -> Vec<GdiTextRun> {
    let mut runs: Vec<GdiTextRun> = Vec::new();
    for ch in text.chars() {
        let family_index = gdi_font_family_for_char(hdc, ch, height, weight);
        if let Some(run) = runs.last_mut() {
            if run.family_index == family_index {
                run.text.push(ch);
                continue;
            }
        }
        runs.push(GdiTextRun {
            family_index,
            text: ch.to_string(),
        });
    }
    runs
}

fn gdi_font_family_for_char(hdc: HDC, ch: char, height: i32, weight: i32) -> usize {
    GDI_FONT_FAMILY_CACHE.with(|cache| {
        let key = (ch, height, weight);
        if let Some(family_index) = cache.borrow().get(&key).copied() {
            return family_index;
        }
        let family_index = unsafe {
            (0..ui_font_family_count())
                .find(|family_index| {
                    gdi_font_family_supports_char(hdc, ch, height, weight, *family_index)
                })
                .unwrap_or(0)
        };
        cache.borrow_mut().insert(key, family_index);
        family_index
    })
}

unsafe fn gdi_font_family_supports_char(
    hdc: HDC,
    ch: char,
    height: i32,
    weight: i32,
    family_index: usize,
) -> bool {
    let Some(font) = create_gdi_font(height, weight, family_index) else {
        return false;
    };
    let old_font = SelectObject(hdc, font.into());
    let mut utf16 = [0u16; 2];
    let units = ch.encode_utf16(&mut utf16);
    let mut glyphs = vec![0u16; units.len()];
    let result = GetGlyphIndicesW(
        hdc,
        PCWSTR(units.as_ptr()),
        units.len() as i32,
        glyphs.as_mut_ptr(),
        GGI_MARK_NONEXISTING_GLYPHS,
    );
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font.into());
    result != u32::MAX && glyphs.iter().all(|glyph| *glyph != 0xFFFF)
}

unsafe fn create_gdi_font(height: i32, weight: i32, family_index: usize) -> Option<HFONT> {
    let font = CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_TT_ONLY_PRECIS,
        windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        ui_font_family_at(family_index),
    );
    (!font.is_invalid()).then_some(font)
}

unsafe fn gdi_text_runs_width(
    hdc: HDC,
    runs: &[GdiTextRun],
    height: i32,
    weight: i32,
    tracking: i32,
) -> i32 {
    runs.iter()
        .filter_map(|run| {
            let font = create_gdi_font(height, weight, run.family_index)?;
            let old_font = SelectObject(hdc, font.into());
            let width = gdi_text_width(hdc, &run.text, tracking);
            let _ = SelectObject(hdc, old_font);
            let _ = DeleteObject(font.into());
            width
        })
        .sum()
}

unsafe fn gdi_text_width(hdc: HDC, text: &str, tracking: i32) -> Option<i32> {
    if text.is_empty() {
        return Some(0);
    }
    let previous_extra = SetTextCharacterExtra(hdc, tracking);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    let measured = GetTextExtentPoint32W(hdc, &wide, &mut size).as_bool();
    let _ = SetTextCharacterExtra(hdc, previous_extra);
    measured.then_some(size.cx)
}

fn colorref(color: Color) -> windows::Win32::Foundation::COLORREF {
    let value = color.0;
    windows::Win32::Foundation::COLORREF(
        ((value & 0xFF) << 16) | (value & 0x00FF00) | ((value >> 16) & 0xFF),
    )
}

fn win_rect(rect: UiRect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn draw_antialiased_rect(hdc: HDC, rect: RECT, style: VisualStyle) -> bool {
    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return false;
        }

        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let path = create_rect_path(rect, style.radius);
        if path.is_null() {
            let _ = GdipDeleteGraphics(graphics);
            return false;
        }

        let ok = fill_and_stroke_path(graphics, path, style);
        let _ = GdipDeletePath(path);
        let _ = GdipDeleteGraphics(graphics);
        ok
    }
}

fn draw_antialiased_ellipse(hdc: HDC, rect: RECT, style: VisualStyle) -> bool {
    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return false;
        }

        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        if let Some(fill) = style.fill {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color_to_argb(fill, style.fill_alpha), &mut brush) != GpOk
                || brush.is_null()
            {
                let _ = GdipDeleteGraphics(graphics);
                return false;
            }
            let _ = GdipFillEllipseI(
                graphics,
                brush as *mut GpBrush,
                rect.left,
                rect.top,
                width,
                height,
            );
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }

        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0 {
                let mut pen: *mut GpPen = std::ptr::null_mut();
                if GdipCreatePen1(
                    color_to_argb(stroke.color, stroke.alpha),
                    stroke.width as f32,
                    UnitPixel,
                    &mut pen,
                ) != GpOk
                    || pen.is_null()
                {
                    let _ = GdipDeleteGraphics(graphics);
                    return false;
                }
                let inset = stroke.width / 2;
                let _ = GdipDrawEllipseI(
                    graphics,
                    pen,
                    rect.left + inset,
                    rect.top + inset,
                    (width - stroke.width).max(1),
                    (height - stroke.width).max(1),
                );
                let _ = GdipDeletePen(pen);
            }
        }

        let _ = GdipDeleteGraphics(graphics);
        true
    }
}

fn fill_and_stroke_path(graphics: *mut GpGraphics, path: *mut GpPath, style: VisualStyle) -> bool {
    unsafe {
        if let Some(fill) = style.fill {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color_to_argb(fill, style.fill_alpha), &mut brush) != GpOk
                || brush.is_null()
            {
                return false;
            }
            let _ = GdipFillPath(graphics, brush as *mut GpBrush, path);
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }

        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0 {
                let mut pen: *mut GpPen = std::ptr::null_mut();
                if GdipCreatePen1(
                    color_to_argb(stroke.color, stroke.alpha),
                    stroke.width as f32,
                    UnitPixel,
                    &mut pen,
                ) != GpOk
                    || pen.is_null()
                {
                    return false;
                }
                let _ = GdipDrawPath(graphics, pen, path);
                let _ = GdipDeletePen(pen);
            }
        }
        true
    }
}

fn fill_and_stroke_ui_path(graphics: *mut GpGraphics, path: *mut GpPath, style: PathStyle) -> bool {
    unsafe {
        if let Some(fill) = style.fill {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color_to_argb(fill, style.fill_alpha), &mut brush) != GpOk
                || brush.is_null()
            {
                return false;
            }
            let _ = GdipFillPath(graphics, brush as *mut GpBrush, path);
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }

        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0 {
                let mut pen: *mut GpPen = std::ptr::null_mut();
                if GdipCreatePen1(
                    color_to_argb(stroke.color, stroke.alpha),
                    stroke.width as f32,
                    UnitPixel,
                    &mut pen,
                ) != GpOk
                    || pen.is_null()
                {
                    return false;
                }
                let _ = GdipDrawPath(graphics, pen, path);
                let _ = GdipDeletePen(pen);
            }
        }
        true
    }
}

fn draw_path(hdc: HDC, path: &UiPath, style: PathStyle) {
    if path.commands().is_empty() {
        return;
    }

    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return;
        }
        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let gp_path = create_ui_path(path);
        if !gp_path.is_null() {
            let _ = fill_and_stroke_ui_path(graphics, gp_path, style);
            let _ = GdipDeletePath(gp_path);
        }
        let _ = GdipDeleteGraphics(graphics);
    }
}

fn create_ui_path(path: &UiPath) -> *mut GpPath {
    unsafe {
        let mut gp_path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut gp_path) != GpOk || gp_path.is_null() {
            return std::ptr::null_mut();
        }

        let mut current: Option<Point> = None;
        let mut figure_start: Option<Point> = None;
        for command in path.commands() {
            let status = match *command {
                UiPathCommand::MoveTo(point) => {
                    current = Some(point);
                    figure_start = Some(point);
                    GpOk
                }
                UiPathCommand::LineTo(point) => {
                    let Some(from) = current else {
                        current = Some(point);
                        figure_start = Some(point);
                        continue;
                    };
                    current = Some(point);
                    GdipAddPathLineI(gp_path, from.x, from.y, point.x, point.y)
                }
                UiPathCommand::QuadraticTo { control, to } => {
                    let Some(from) = current else {
                        current = Some(to);
                        figure_start = Some(to);
                        continue;
                    };
                    let control1 = Point::new(
                        from.x + ((control.x - from.x) * 2) / 3,
                        from.y + ((control.y - from.y) * 2) / 3,
                    );
                    let control2 = Point::new(
                        to.x + ((control.x - to.x) * 2) / 3,
                        to.y + ((control.y - to.y) * 2) / 3,
                    );
                    current = Some(to);
                    GdipAddPathBezierI(
                        gp_path, from.x, from.y, control1.x, control1.y, control2.x, control2.y,
                        to.x, to.y,
                    )
                }
                UiPathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    let Some(from) = current else {
                        current = Some(to);
                        figure_start = Some(to);
                        continue;
                    };
                    current = Some(to);
                    GdipAddPathBezierI(
                        gp_path, from.x, from.y, control1.x, control1.y, control2.x, control2.y,
                        to.x, to.y,
                    )
                }
                UiPathCommand::Close => {
                    current = figure_start;
                    GdipClosePathFigure(gp_path)
                }
            };
            if status != GpOk {
                let _ = GdipDeletePath(gp_path);
                return std::ptr::null_mut();
            }
        }

        gp_path
    }
}

fn create_rect_path(rect: RECT, radius: i32) -> *mut GpPath {
    if radius > 0 {
        return create_rounded_rect_path(rect, radius);
    }

    unsafe {
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != GpOk || path.is_null() {
            return std::ptr::null_mut();
        }

        let statuses = [
            GdipAddPathLineI(path, rect.left, rect.top, rect.right, rect.top),
            GdipAddPathLineI(path, rect.right, rect.top, rect.right, rect.bottom),
            GdipAddPathLineI(path, rect.right, rect.bottom, rect.left, rect.bottom),
            GdipAddPathLineI(path, rect.left, rect.bottom, rect.left, rect.top),
            GdipClosePathFigure(path),
        ];
        if statuses.iter().any(|status| *status != GpOk) {
            let _ = GdipDeletePath(path);
            return std::ptr::null_mut();
        }
        path
    }
}

fn create_rounded_rect_path(rect: RECT, radius: i32) -> *mut GpPath {
    unsafe {
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != GpOk || path.is_null() {
            return std::ptr::null_mut();
        }

        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        let diameter = (radius * 2).min(width).min(height).max(2);
        let line_radius = diameter / 2;
        let right = rect.right - diameter;
        let bottom = rect.bottom - diameter;

        let statuses = [
            GdipAddPathArcI(path, rect.left, rect.top, diameter, diameter, 180.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.left + line_radius,
                rect.top,
                rect.right - line_radius,
                rect.top,
            ),
            GdipAddPathArcI(path, right, rect.top, diameter, diameter, 270.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.right,
                rect.top + line_radius,
                rect.right,
                rect.bottom - line_radius,
            ),
            GdipAddPathArcI(path, right, bottom, diameter, diameter, 0.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.right - line_radius,
                rect.bottom,
                rect.left + line_radius,
                rect.bottom,
            ),
            GdipAddPathArcI(path, rect.left, bottom, diameter, diameter, 90.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.left,
                rect.bottom - line_radius,
                rect.left,
                rect.top + line_radius,
            ),
            GdipClosePathFigure(path),
        ];

        if statuses.iter().any(|status| *status != GpOk) {
            let _ = GdipDeletePath(path);
            return std::ptr::null_mut();
        }

        path
    }
}

fn color_to_argb(color: Color, alpha: u8) -> u32 {
    let red = (color.0 >> 16) & 0xFF;
    let green = (color.0 >> 8) & 0xFF;
    let blue = color.0 & 0xFF;
    ((alpha as u32) << 24) | (red << 16) | (green << 8) | blue
}
