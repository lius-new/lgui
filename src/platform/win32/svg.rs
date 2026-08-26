use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
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

use crate::core::{Color, IconStyle, UiRect};

static SVG_REGISTRY: OnceLock<SvgIconRegistry> = OnceLock::new();
static SVG_FONT_REGISTRY: OnceLock<SvgFontRegistry> = OnceLock::new();
static SVG_FONTDB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub enum SvgIconSource {
    Text(&'static str),
    Bytes(&'static [u8]),
    Icon(icondata::Icon),
}

impl SvgIconSource {
    fn to_svg(self) -> Option<Cow<'static, str>> {
        match self {
            SvgIconSource::Text(svg) => Some(Cow::Borrowed(svg)),
            SvgIconSource::Bytes(bytes) => std::str::from_utf8(bytes).ok().map(Cow::Borrowed),
            SvgIconSource::Icon(icon) => Some(Cow::Owned(icondata_to_svg(icon))),
        }
    }
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

#[derive(Default)]
pub struct SvgIconRegistry {
    icons: HashMap<&'static str, SvgIconSource>,
}

impl SvgIconRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_icon(mut self, key: &'static str, svg: impl Into<SvgIconSource>) -> Self {
        self.icons.insert(key, svg.into());
        self
    }

    pub fn with_icon_bytes(mut self, key: &'static str, bytes: &'static [u8]) -> Self {
        self.icons.insert(key, SvgIconSource::Bytes(bytes));
        self
    }

    pub fn with_icon_text(mut self, key: &'static str, svg: &'static str) -> Self {
        self.icons.insert(key, SvgIconSource::Text(svg));
        self
    }

    fn resolve(&self, key: &str) -> Option<Cow<'static, str>> {
        self.icons
            .get(key)
            .and_then(|source| source.to_svg())
            .or_else(|| builtin_svg(key))
    }
}

impl From<&'static str> for SvgIconSource {
    fn from(value: &'static str) -> Self {
        Self::Text(value)
    }
}

impl From<&'static [u8]> for SvgIconSource {
    fn from(value: &'static [u8]) -> Self {
        Self::Bytes(value)
    }
}

impl From<icondata::Icon> for SvgIconSource {
    fn from(value: icondata::Icon) -> Self {
        Self::Icon(value)
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

pub struct SvgBitmap {
    pub width: i32,
    pub height: i32,
    pub premultiplied_bgra: Vec<u8>,
}

thread_local! {
    static SVG_CACHE: RefCell<HashMap<SvgCacheKey, SvgBitmap>> = RefCell::new(HashMap::new());
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
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let cache_key = SvgCacheKey {
        key,
        width,
        height,
        color: style.color.0,
        alpha: style.alpha,
    };
    SVG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&cache_key) {
            let Some(bitmap) = rasterize_icon(cache_key) else {
                return None;
            };
            cache.insert(cache_key, bitmap);
        }
        cache.get(&cache_key).map(|bitmap| SvgBitmap {
            width: bitmap.width,
            height: bitmap.height,
            premultiplied_bgra: bitmap.premultiplied_bgra.clone(),
        })
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
            rect.left,
            rect.top,
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

fn resolve_svg(key: &str) -> Option<Cow<'static, str>> {
    SVG_REGISTRY
        .get()
        .and_then(|registry| registry.resolve(key))
        .or_else(|| builtin_svg(key))
}

fn builtin_svg(key: &str) -> Option<Cow<'static, str>> {
    match key {
        "copy" => Some(Cow::Borrowed(COPY)),
        "arrow-left" => Some(Cow::Borrowed(ARROW_LEFT)),
        "close" => Some(Cow::Owned(icondata_to_svg(icondata::LuX))),
        "minus" => Some(Cow::Owned(icondata_to_svg(icondata::LuMinus))),
        _ => None,
    }
}

fn icondata_to_svg(icon: icondata::Icon) -> String {
    let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    push_svg_attr(&mut svg, "style", icon.style);
    push_svg_attr(&mut svg, "x", icon.x);
    push_svg_attr(&mut svg, "y", icon.y);
    push_svg_attr(&mut svg, "width", icon.width);
    push_svg_attr(&mut svg, "height", icon.height);
    push_svg_attr(&mut svg, "viewBox", icon.view_box);
    push_svg_attr(&mut svg, "stroke-linecap", icon.stroke_linecap);
    push_svg_attr(&mut svg, "stroke-linejoin", icon.stroke_linejoin);
    push_svg_attr(&mut svg, "stroke-width", icon.stroke_width);
    push_svg_attr(&mut svg, "stroke", icon.stroke);
    push_svg_attr(&mut svg, "fill", icon.fill);
    svg.push_str(" opacity=\"currentOpacity\">");
    svg.push_str(icon.data);
    svg.push_str("</svg>");
    svg
}

fn push_svg_attr(svg: &mut String, name: &str, value: Option<&'static str>) {
    if let Some(value) = value {
        svg.push(' ');
        svg.push_str(name);
        svg.push_str("=\"");
        svg.push_str(value);
        svg.push('"');
    }
}

const COPY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" opacity="currentOpacity"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>"#;
const ARROW_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" opacity="currentOpacity"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>"#;
