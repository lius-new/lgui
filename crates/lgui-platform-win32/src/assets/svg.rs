use std::{
    borrow::Cow,
    cell::RefCell,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use tiny_skia::{Pixmap, Transform};

use lgui_assets::icons::{builtin_svg, SvgIconRegistry};
use lgui_core::core::{Color, IconStyle, UiRect};

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

pub(crate) fn resolve_svg(key: &str) -> Option<Cow<'static, str>> {
    SVG_REGISTRY
        .get()
        .and_then(|registry| registry.resolve(key))
        .or_else(|| builtin_svg(key))
}
