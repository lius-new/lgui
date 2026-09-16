use std::{
    borrow::Cow,
    cell::RefCell,
    ffi::c_void,
    ptr::null_mut,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use tiny_skia::{Pixmap, Transform};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HDC,
};

use lgui_core::{
    core::{Color, IconStyle, UiRect},
    icons::{builtin_svg, SvgIconRegistry},
};

static SVG_REGISTRY: OnceLock<SvgIconRegistry> = OnceLock::new();
static SVG_FONT_REGISTRY: OnceLock<SvgFontRegistry> = OnceLock::new();
static SVG_FONTDB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();

fn svg_telemetry() -> &'static lgui_core::memory::CacheTelemetry {
    static TELEMETRY: OnceLock<lgui_core::memory::CacheTelemetry> = OnceLock::new();
    TELEMETRY.get_or_init(Default::default)
}

#[derive(Default)]
pub struct SvgFontRegistry {
    fonts: Vec<&'static [u8]>,
}

impl SvgFontRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_font_bytes(mut self, bytes: &'static [u8]) -> Self {
        self.fonts.push(bytes);
        self
    }
}

pub fn install_svg_icon_registry(registry: SvgIconRegistry) -> bool {
    let installed = SVG_REGISTRY.set(registry).is_ok();
    if installed {
        SVG_CACHE.with(|cache| cache.borrow_mut().clear());
    }
    installed
}

pub fn install_svg_font_registry(registry: SvgFontRegistry) -> bool {
    let installed = SVG_FONT_REGISTRY.set(registry).is_ok();
    if installed {
        SVG_CACHE.with(|cache| cache.borrow_mut().clear());
    }
    installed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SvgCacheKey {
    key: &'static str,
    width: i32,
    height: i32,
    color: u32,
    alpha: u8,
}

#[derive(Clone)]
pub struct SvgBitmap {
    pub width: i32,
    pub height: i32,
    pub premultiplied_bgra: Vec<u8>,
}

thread_local! {
    static SVG_CACHE: RefCell<lgui_core::memory::LruCache<SvgCacheKey, SvgBitmap>> = RefCell::new(
        lgui_core::memory::LruCache::new(
            0,
            lgui_core::memory::ResourceClass::Cache,
            svg_telemetry().clone(),
        )
    );
}

#[cfg(feature = "backend-win32")]
pub(crate) fn svg_bitmap_cache_usage() -> lgui_core::memory::CacheUsage {
    svg_telemetry().snapshot()
}

#[cfg(feature = "backend-win32")]
pub(crate) fn trim_svg_bitmap_cache(target_bytes: usize) -> usize {
    SVG_CACHE.with(|cache| cache.borrow_mut().trim_to(target_bytes))
}

#[cfg(feature = "backend-win32")]
pub(crate) fn set_svg_bitmap_cache_budget(budget_bytes: usize) {
    SVG_CACHE.with(|cache| cache.borrow_mut().set_budget(budget_bytes));
}

pub fn draw_svg_icon(hdc: HDC, key: &'static str, rect: UiRect, style: IconStyle) {
    if let Some(bitmap) = rasterize_svg_icon_bgra(key, rect, style) {
        draw_bitmap(hdc, rect, &bitmap);
    }
}

pub fn rasterize_svg_icon_bgra(
    key: &'static str,
    rect: UiRect,
    style: IconStyle,
) -> Option<SvgBitmap> {
    let width = rect.width().ceil().max(1.0) as i32;
    let height = rect.height().ceil().max(1.0) as i32;
    let cache_key = SvgCacheKey {
        key,
        width,
        height,
        color: style.color.0,
        alpha: style.alpha,
    };
    SVG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_touch(&cache_key) {
            let Some(bitmap) = rasterize_icon(cache_key) else {
                return None;
            };
            let bytes = bitmap.premultiplied_bgra.len();
            if !cache.insert(cache_key, bitmap, bytes) {
                return rasterize_icon(cache_key);
            }
        }
        cache.get(&cache_key).cloned()
    })
}

fn rasterize_icon(key: SvgCacheKey) -> Option<SvgBitmap> {
    let start = Instant::now();
    let svg = resolve_svg(key.key)?;
    let tinted = tint_svg(svg.as_ref(), Color(key.color), key.alpha);
    let mut options = usvg::Options::default();
    options.fontdb = svg_fontdb().clone();
    let tree = usvg::Tree::from_str(&tinted, &options).ok()?;
    let mut pixmap = Pixmap::new(key.width as u32, key.height as u32)?;
    let size = tree.size();
    let scale_x = key.width as f32 / size.width();
    let scale_y = key.height as f32 / size.height();
    let transform = Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let bitmap = SvgBitmap {
        width: key.width,
        height: key.height,
        premultiplied_bgra: rgba_to_premultiplied_bgra(pixmap.data()),
    };
    trace_duration("svg.rasterize_icon", start.elapsed());
    Some(bitmap)
}

fn svg_fontdb() -> &'static Arc<usvg::fontdb::Database> {
    SVG_FONTDB.get_or_init(|| {
        let start = Instant::now();
        let mut database = usvg::fontdb::Database::new();
        if let Some(registry) = SVG_FONT_REGISTRY.get() {
            for font in &registry.fonts {
                database.load_font_data(font.to_vec());
            }
        }
        trace_duration("svg.load_fonts", start.elapsed());
        Arc::new(database)
    })
}

fn trace_duration(label: &str, duration: Duration) {
    let _ = (label, duration);
}

fn tint_svg(svg: &str, color: Color, alpha: u8) -> String {
    let hex = format!("#{:06X}", color.0);
    let opacity = format!("{:.3}", alpha as f32 / 255.0);
    svg.replace("currentColor", &hex)
        .replace("currentOpacity", &opacity)
}

fn rgba_to_premultiplied_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        let alpha = pixel[3] as u32;
        bgra.push(((pixel[2] as u32 * alpha + 127) / 255) as u8);
        bgra.push(((pixel[1] as u32 * alpha + 127) / 255) as u8);
        bgra.push(((pixel[0] as u32 * alpha + 127) / 255) as u8);
        bgra.push(pixel[3]);
    }
    bgra
}

fn draw_bitmap(hdc: HDC, rect: UiRect, bitmap: &SvgBitmap) {
    unsafe {
        let memory_dc = CreateCompatibleDC(Some(hdc));
        if memory_dc.is_invalid() {
            return;
        }

        let mut bits: *mut c_void = null_mut();
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bitmap.width,
                biHeight: -bitmap.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let Ok(dib) = CreateDIBSection(Some(hdc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(memory_dc);
            return;
        };
        if dib.is_invalid() || bits.is_null() {
            let _ = DeleteDC(memory_dc);
            return;
        }

        std::ptr::copy_nonoverlapping(
            bitmap.premultiplied_bgra.as_ptr(),
            bits as *mut u8,
            bitmap.premultiplied_bgra.len(),
        );

        let old_bitmap = SelectObject(memory_dc, dib.into());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = AlphaBlend(
            hdc,
            rect.left.round() as i32,
            rect.top.round() as i32,
            bitmap.width,
            bitmap.height,
            memory_dc,
            0,
            0,
            bitmap.width,
            bitmap.height,
            blend,
        );
        let _ = SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(memory_dc);
    }
}

pub(crate) fn resolve_svg(key: &str) -> Option<Cow<'static, str>> {
    SVG_REGISTRY
        .get()
        .and_then(|registry| registry.resolve(key))
        .or_else(|| builtin_svg(key))
}
