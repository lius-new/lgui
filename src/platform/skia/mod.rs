use std::{
    cell::RefCell,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    application::GraphicsPreference,
    assets::render_resources,
    core::{
        BackdropBlurStyle, Color, CompositingLayerBackground, ImageFit, LayerTransform, PathStyle,
        PhysicalRect, Scene, ScenePrimitive, StaticLayerBackground, StaticLayerCachePolicy,
        StaticLayerSource, Stroke, TextAlign, TextStyle, UiImageSource, UiPath, UiPathCommand,
        UiRect, VisualStyle,
    },
    renderer::{FrameInfo, MemoryPressure},
};
use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, RectHeightStyle, RectWidthStyle,
    TextAlign as SkTextAlign, TextDirection as SkTextDirection, TextStyle as SkTextStyle,
    TypefaceFontProvider,
};
use skia_safe::{
    surfaces, AlphaType, BlendMode, Canvas, Color as SkColor, Color4f, ColorType, Data, FontMgr,
    FontStyle, Image, ImageInfo, Paint, PaintStyle, Path, PathBuilder, RRect, Rect,
    SamplingOptions, Surface, TileMode,
};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) const DEFAULT_CACHE_BUDGET: usize = 96 * 1024 * 1024;

pub(crate) const fn gpu_cache_budget(total: usize) -> usize {
    total.saturating_mul(2) / 3
}

pub(crate) const fn cpu_cache_budget(total: usize) -> usize {
    total.saturating_sub(gpu_cache_budget(total))
}

pub(crate) fn with_gpu_cache_usage(
    mut stats: SkiaCacheStats,
    context: &skia_safe::gpu::DirectContext,
) -> SkiaCacheStats {
    let usage = context.resource_cache_usage();
    stats.budget_bytes = stats
        .budget_bytes
        .saturating_add(context.resource_cache_limit());
    stats.resident_bytes = stats.resident_bytes.saturating_add(usage.resource_bytes);
    stats.entries = stats.entries.saturating_add(usage.resource_count);
    stats
}

pub fn probe_skia_support(preference: GraphicsPreference) -> Result<(), String> {
    match preference {
        GraphicsPreference::Auto | GraphicsPreference::Software => {
            surfaces::raster_n32_premul((1, 1))
                .map(|_| ())
                .ok_or_else(|| "Skia could not create a raster surface".to_owned())
        }
        #[cfg(feature = "renderer-skia-gl")]
        GraphicsPreference::OpenGl => surfaces::raster_n32_premul((1, 1))
            .map(|_| ())
            .ok_or_else(|| "Skia could not create a raster surface".to_owned()),
        #[cfg(not(feature = "renderer-skia-gl"))]
        GraphicsPreference::OpenGl => Err("OpenGL is not enabled on this target".to_owned()),
        #[cfg(all(
            feature = "renderer-skia-vulkan",
            any(target_os = "windows", target_os = "linux")
        ))]
        GraphicsPreference::Vulkan => unsafe { ash::Entry::load() }
            .map(|_| ())
            .map_err(|error| format!("load Vulkan runtime: {error}")),
        #[cfg(not(all(
            feature = "renderer-skia-vulkan",
            any(target_os = "windows", target_os = "linux")
        )))]
        GraphicsPreference::Vulkan => Err("Vulkan is not enabled on this build".to_owned()),
        #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
        GraphicsPreference::Metal => surfaces::raster_n32_premul((1, 1))
            .map(|_| ())
            .ok_or_else(|| "Skia could not initialize Metal support".to_owned()),
        #[cfg(not(all(feature = "renderer-skia-metal", target_os = "macos")))]
        GraphicsPreference::Metal => Err("Metal is not enabled on this target".to_owned()),
    }
}

struct SkiaTextSystem;

thread_local! {
    static SKIA_TEXT_FONTS: RefCell<FontCollection> = RefCell::new(skia_font_collection());
}

impl crate::text::TextSystem for SkiaTextSystem {
    fn measure(
        &self,
        request: &crate::text::TextMeasureRequest<'_>,
    ) -> Option<crate::text::TextMetrics> {
        let layout_request = crate::text::TextLayoutRequest::single_line(
            request.text,
            request.bounds,
            request.font_height,
            request.font_weight,
        );
        self.layout(&layout_request)
            .map(|layout| crate::text::TextMetrics {
                width: layout.width,
            })
    }

    fn layout(
        &self,
        request: &crate::text::TextLayoutRequest<'_>,
    ) -> Option<crate::text::TextLayout> {
        SKIA_TEXT_FONTS.with(|fonts| {
            let mut paragraph = build_skia_paragraph(request, fonts.borrow().clone(), None);
            paragraph.layout(request.bounds.width().max(1.0));
            Some(portable_text_layout(&paragraph, request))
        })
    }
}

pub(crate) fn skia_text_system_handle() -> crate::text::TextSystemHandle {
    crate::text::TextSystemHandle::new(SkiaTextSystem)
}

fn skia_font_collection() -> FontCollection {
    let mut collection = FontCollection::new();
    collection.set_default_font_manager(FontMgr::default(), None);
    let assets = crate::text::font_assets();
    if !assets.is_empty() {
        let system = FontMgr::default();
        let mut provider = TypefaceFontProvider::new();
        for asset in assets.iter() {
            if let Some(typeface) =
                system.new_from_data(&Data::new_copy(asset.bytes.as_slice()), None)
            {
                provider.register_typeface(typeface, asset.family_alias.as_deref());
            }
        }
        let manager: FontMgr = provider.into();
        collection.set_dynamic_font_manager(manager);
    }
    collection.paragraph_cache_mut().turn_on(false);
    collection
}

fn build_skia_paragraph(
    request: &crate::text::TextLayoutRequest<'_>,
    fonts: FontCollection,
    color: Option<SkColor>,
) -> Paragraph {
    let direction = resolved_text_direction(request.text, request.direction);
    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style
        .set_text_align(match request.align {
            TextAlign::Left => SkTextAlign::Left,
            TextAlign::Center => SkTextAlign::Center,
            TextAlign::Right => SkTextAlign::Right,
        })
        .set_text_direction(match direction {
            crate::text::TextDirection::RightToLeft => SkTextDirection::RTL,
            _ => SkTextDirection::LTR,
        })
        .set_max_lines(request.max_lines);

    let base_style = skia_paragraph_text_style(
        request.font_height,
        request.font_weight,
        request.font_width,
        request.font_slant,
        request.font_families,
        request.locale,
        request.tracking,
        request.line_height,
        request.features,
        color,
    );
    paragraph_style.set_text_style(&base_style);
    let mut builder = ParagraphBuilder::new(&paragraph_style, fonts);
    if request.spans.is_empty() {
        builder.push_style(&base_style).add_text(request.text).pop();
    } else {
        let char_boundaries = char_byte_boundaries(request.text);
        let mut cursor = 0;
        for span in request.spans {
            let start = span
                .range
                .start
                .max(cursor)
                .min(char_boundaries.len().saturating_sub(1));
            let end = span
                .range
                .end
                .max(start)
                .min(char_boundaries.len().saturating_sub(1));
            if start > cursor {
                builder
                    .push_style(&base_style)
                    .add_text(&request.text[char_boundaries[cursor]..char_boundaries[start]])
                    .pop();
            }
            if end > start {
                let style = skia_paragraph_text_style(
                    span.font_height.unwrap_or(request.font_height),
                    span.font_weight.unwrap_or(request.font_weight),
                    span.font_width.unwrap_or(request.font_width),
                    span.font_slant.unwrap_or(request.font_slant),
                    if span.font_families.is_empty() {
                        request.font_families
                    } else {
                        span.font_families
                    },
                    span.locale.unwrap_or(request.locale),
                    span.tracking.unwrap_or(request.tracking),
                    request.line_height,
                    if span.features.is_empty() {
                        request.features
                    } else {
                        span.features
                    },
                    color,
                );
                builder
                    .push_style(&style)
                    .add_text(&request.text[char_boundaries[start]..char_boundaries[end]])
                    .pop();
                cursor = cursor.max(end);
            }
        }
        if cursor < char_boundaries.len().saturating_sub(1) {
            builder
                .push_style(&base_style)
                .add_text(&request.text[char_boundaries[cursor]..])
                .pop();
        }
    }
    builder.build()
}

#[allow(clippy::too_many_arguments)]
fn skia_paragraph_text_style(
    font_height: f32,
    font_weight: i32,
    font_width: crate::text::TextFontWidth,
    font_slant: crate::text::TextFontSlant,
    requested_families: &[&str],
    locale: &str,
    tracking: f32,
    line_height: Option<f32>,
    features: &[crate::text::TextFeature<'_>],
    color: Option<SkColor>,
) -> SkTextStyle {
    let size = font_height.abs().max(1.0);
    let mut style = SkTextStyle::new();
    let families = if requested_families.is_empty() {
        crate::text::font_families()
    } else {
        requested_families
    };
    style
        .set_font_families(families)
        .set_font_size(size)
        .set_font_style(FontStyle::new(
            skia_safe::font_style::Weight::from(font_weight.clamp(1, 1000)),
            skia_safe::font_style::Width::from(font_width.0.clamp(1, 9)),
            match font_slant {
                crate::text::TextFontSlant::Upright => skia_safe::font_style::Slant::Upright,
                crate::text::TextFontSlant::Italic => skia_safe::font_style::Slant::Italic,
                crate::text::TextFontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
            },
        ))
        .set_letter_spacing(tracking);
    if !locale.is_empty() {
        style.set_locale(locale);
    }
    if let Some(line_height) = line_height.filter(|height| *height > 0.0) {
        style
            .set_height(line_height / size)
            .set_height_override(true);
    }
    if let Some(color) = color {
        style.set_color(color);
    }
    for feature in features {
        style.add_font_feature(feature.name, feature.value);
    }
    style
}

fn resolved_text_direction(
    text: &str,
    requested: crate::text::TextDirection,
) -> crate::text::TextDirection {
    if requested != crate::text::TextDirection::Auto {
        return requested;
    }
    BidiInfo::new(text, None)
        .paragraphs
        .first()
        .map(|paragraph| {
            if paragraph.level.is_rtl() {
                crate::text::TextDirection::RightToLeft
            } else {
                crate::text::TextDirection::LeftToRight
            }
        })
        .unwrap_or(crate::text::TextDirection::LeftToRight)
}

fn portable_text_layout(
    paragraph: &Paragraph,
    request: &crate::text::TextLayoutRequest<'_>,
) -> crate::text::TextLayout {
    let utf16_boundaries = char_utf16_boundaries(request.text);
    let byte_boundaries = char_byte_boundaries(request.text);
    let content_height = paragraph.height();
    let offset_y = request.bounds.top
        + match request.vertical_align {
            crate::text::TextVerticalAlign::Top => 0.0,
            crate::text::TextVerticalAlign::Center => {
                ((request.bounds.height() - content_height) * 0.5).max(0.0)
            }
            crate::text::TextVerticalAlign::Bottom => {
                (request.bounds.height() - content_height).max(0.0)
            }
        };
    let offset_x = request.bounds.left;
    let lines = paragraph
        .get_line_metrics()
        .into_iter()
        .map(|line| crate::text::TextLineMetrics {
            range: utf16_range_to_char_range(&utf16_boundaries, line.start_index..line.end_index),
            bounds: UiRect::new(
                offset_x + line.left as f32,
                offset_y + (line.baseline - line.ascent) as f32,
                offset_x + (line.left + line.width) as f32,
                offset_y + (line.baseline + line.descent) as f32,
            ),
            baseline: offset_y + line.baseline as f32,
            hard_break: line.hard_break,
        })
        .collect::<Vec<_>>();
    let mut clusters = Vec::new();
    for (start_byte, grapheme) in request.text.grapheme_indices(true) {
        let end_byte = start_byte + grapheme.len();
        let range = byte_range_to_char_range(&byte_boundaries, start_byte..end_byte);
        let utf16_range = utf16_boundaries[range.start]..utf16_boundaries[range.end];
        for text_box in
            paragraph.get_rects_for_range(utf16_range, RectHeightStyle::Max, RectWidthStyle::Tight)
        {
            clusters.push(crate::text::TextCluster {
                range: range.clone(),
                bounds: offset_skia_rect(text_box.rect, offset_x, offset_y),
                direction: if text_box.direct == SkTextDirection::RTL {
                    crate::text::TextDirection::RightToLeft
                } else {
                    crate::text::TextDirection::LeftToRight
                },
            });
        }
    }
    let mut carets = Vec::with_capacity(utf16_boundaries.len() * 2);
    for char_index in 0..utf16_boundaries.len() {
        let utf16_index = utf16_boundaries[char_index];
        if char_index > 0 {
            if let Some(rect) = paragraph_caret_rect(
                paragraph,
                utf16_boundaries[char_index - 1]..utf16_index,
                false,
                offset_x,
                offset_y,
            ) {
                carets.push((char_index, crate::text::TextAffinity::Upstream, rect));
            }
        }
        if char_index + 1 < utf16_boundaries.len() {
            if let Some(rect) = paragraph_caret_rect(
                paragraph,
                utf16_index..utf16_boundaries[char_index + 1],
                true,
                offset_x,
                offset_y,
            ) {
                carets.push((char_index, crate::text::TextAffinity::Downstream, rect));
            }
        }
    }
    for char_index in 0..utf16_boundaries.len() {
        if carets.iter().any(|(index, _, _)| *index == char_index) {
            continue;
        }
        if let Some(cluster) = clusters
            .iter()
            .find(|cluster| cluster.range.start == char_index || cluster.range.end == char_index)
        {
            let at_start = cluster.range.start == char_index;
            let x = match (cluster.direction, at_start) {
                (crate::text::TextDirection::RightToLeft, true) => cluster.bounds.right,
                (crate::text::TextDirection::RightToLeft, false) => cluster.bounds.left,
                (_, true) => cluster.bounds.left,
                (_, false) => cluster.bounds.right,
            };
            carets.push((
                char_index,
                if at_start {
                    crate::text::TextAffinity::Downstream
                } else {
                    crate::text::TextAffinity::Upstream
                },
                UiRect::new(x, cluster.bounds.top, x, cluster.bounds.bottom),
            ));
        } else if let Some(line) = lines
            .iter()
            .find(|line| char_index >= line.range.start && char_index <= line.range.end)
        {
            let x = if char_index == line.range.end {
                line.bounds.right
            } else {
                line.bounds.left
            };
            carets.push((
                char_index,
                crate::text::TextAffinity::Downstream,
                UiRect::new(x, line.bounds.top, x, line.bounds.bottom),
            ));
        }
    }
    crate::text::TextLayout::new(
        paragraph.longest_line(),
        content_height,
        paragraph.did_exceed_max_lines(),
        lines,
        clusters,
        carets,
    )
}

fn paragraph_caret_rect(
    paragraph: &Paragraph,
    byte_range: std::ops::Range<usize>,
    at_start: bool,
    offset_x: f32,
    offset_y: f32,
) -> Option<UiRect> {
    let text_box = paragraph
        .get_rects_for_range(byte_range, RectHeightStyle::Max, RectWidthStyle::Tight)
        .into_iter()
        .next()?;
    let leading = if text_box.direct == SkTextDirection::RTL {
        text_box.rect.right
    } else {
        text_box.rect.left
    };
    let trailing = if text_box.direct == SkTextDirection::RTL {
        text_box.rect.left
    } else {
        text_box.rect.right
    };
    let x = if at_start { leading } else { trailing } + offset_x;
    Some(UiRect::new(
        x,
        text_box.rect.top + offset_y,
        x,
        text_box.rect.bottom + offset_y,
    ))
}

fn offset_skia_rect(rect: Rect, offset_x: f32, offset_y: f32) -> UiRect {
    UiRect::new(
        rect.left + offset_x,
        rect.top + offset_y,
        rect.right + offset_x,
        rect.bottom + offset_y,
    )
}

fn char_byte_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn char_utf16_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(text.chars().count() + 1);
    let mut offset = 0;
    boundaries.push(offset);
    for ch in text.chars() {
        offset += ch.len_utf16();
        boundaries.push(offset);
    }
    boundaries
}

fn byte_range_to_char_range(
    boundaries: &[usize],
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    byte_to_char_index(boundaries, range.start)..byte_to_char_index(boundaries, range.end)
}

fn byte_to_char_index(boundaries: &[usize], byte: usize) -> usize {
    boundaries.partition_point(|boundary| *boundary < byte)
}

fn utf16_range_to_char_range(
    boundaries: &[usize],
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    utf16_to_char_index(boundaries, range.start)..utf16_to_char_index(boundaries, range.end)
}

fn utf16_to_char_index(boundaries: &[usize], offset: usize) -> usize {
    boundaries.partition_point(|boundary| *boundary < offset)
}

struct CachedImage {
    image: Image,
    bytes: usize,
    used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ParagraphCacheKey {
    text: String,
    width: u32,
    height: u32,
    style: TextStyle,
    families: Vec<&'static str>,
}

struct CachedParagraph {
    paragraph: Paragraph,
    bytes: usize,
    used: u64,
}

pub(crate) struct SkiaCache {
    entries: HashMap<String, CachedImage>,
    paragraphs: HashMap<ParagraphCacheKey, CachedParagraph>,
    fonts: FontCollection,
    resident_bytes: usize,
    budget_bytes: usize,
    generation: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    text_hits: u64,
    text_misses: u64,
    text_evictions: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SkiaCacheStats {
    pub budget_bytes: usize,
    pub resident_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub text_resident_bytes: usize,
    pub text_entries: usize,
    pub text_hits: u64,
    pub text_misses: u64,
    pub text_evictions: u64,
    pub largest_entry_bytes: usize,
    pub largest_text_entry_bytes: usize,
}

impl SkiaCache {
    pub(crate) fn new(budget_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            paragraphs: HashMap::new(),
            fonts: skia_font_collection(),
            resident_bytes: 0,
            budget_bytes,
            generation: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            text_hits: 0,
            text_misses: 0,
            text_evictions: 0,
        }
    }

    fn draw_text(&mut self, canvas: &Canvas, rect: UiRect, text: &str, style: TextStyle) {
        let key = ParagraphCacheKey {
            text: text.to_owned(),
            width: rect.width().to_bits(),
            height: rect.height().to_bits(),
            style,
            families: crate::text::font_families().to_vec(),
        };
        if let Some(entry) = self.paragraphs.get_mut(&key) {
            self.text_hits = self.text_hits.saturating_add(1);
            entry.used = self.generation;
            paint_cached_paragraph(canvas, rect, &entry.paragraph);
            return;
        }
        self.text_misses = self.text_misses.saturating_add(1);
        let request = scene_text_layout_request(text, rect, style);
        let mut paragraph = build_skia_paragraph(
            &request,
            self.fonts.clone(),
            Some(sk_color(style.color, style.alpha)),
        );
        paragraph.layout(rect.width().max(1.0));
        paint_cached_paragraph(canvas, rect, &paragraph);
        let bytes = paragraph_cache_entry_bytes(text, &paragraph);
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.paragraphs.insert(
            key,
            CachedParagraph {
                paragraph,
                bytes,
                used: self.generation,
            },
        );
        self.evict_to_budget();
    }

    fn get(&mut self, key: &str) -> Option<Image> {
        let Some(entry) = self.entries.get_mut(key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        entry.used = self.generation;
        Some(entry.image.clone())
    }

    fn insert(&mut self, key: String, image: Image) -> Image {
        let bytes = image.width().max(0) as usize * image.height().max(0) as usize * 4;
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.bytes);
        }
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            CachedImage {
                image: image.clone(),
                bytes,
                used: self.generation,
            },
        );
        self.evict_to_budget();
        image
    }

    pub(crate) fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn evict_to_budget(&mut self) {
        while self.resident_bytes > self.budget_bytes {
            let image = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, entry)| (key.clone(), entry.used));
            let paragraph = self
                .paragraphs
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, entry)| (key.clone(), entry.used));
            if image.is_none() && paragraph.is_none() {
                break;
            }
            if paragraph.as_ref().is_some_and(|(_, paragraph_used)| {
                image
                    .as_ref()
                    .is_none_or(|(_, image_used)| paragraph_used <= image_used)
            }) {
                if let Some((key, _)) = paragraph {
                    if let Some(entry) = self.paragraphs.remove(&key) {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                        self.text_evictions = self.text_evictions.saturating_add(1);
                    }
                }
            } else if let Some((key, _)) = image {
                if let Some(entry) = self.entries.remove(&key) {
                    self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    self.evictions = self.evictions.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Moderate => {
                let current = self.generation;
                let before = self.entries.len();
                self.entries.retain(|_, entry| {
                    let keep = current.wrapping_sub(entry.used) <= 2;
                    if !keep {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    }
                    keep
                });
                self.evictions = self
                    .evictions
                    .saturating_add(before.saturating_sub(self.entries.len()) as u64);
                let before = self.paragraphs.len();
                self.paragraphs.retain(|_, entry| {
                    let keep = current.wrapping_sub(entry.used) <= 2;
                    if !keep {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    }
                    keep
                });
                self.text_evictions = self
                    .text_evictions
                    .saturating_add(before.saturating_sub(self.paragraphs.len()) as u64);
                self.fonts.clear_caches();
            }
            MemoryPressure::Critical => {
                self.evictions = self.evictions.saturating_add(self.entries.len() as u64);
                self.text_evictions = self
                    .text_evictions
                    .saturating_add(self.paragraphs.len() as u64);
                self.entries.clear();
                self.paragraphs.clear();
                self.fonts.clear_caches();
                self.resident_bytes = 0;
            }
        }
    }

    pub(crate) fn stats(&self) -> SkiaCacheStats {
        let text_resident_bytes = self.paragraphs.values().map(|entry| entry.bytes).sum();
        SkiaCacheStats {
            budget_bytes: self.budget_bytes,
            resident_bytes: self.resident_bytes,
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            text_resident_bytes,
            text_entries: self.paragraphs.len(),
            text_hits: self.text_hits,
            text_misses: self.text_misses,
            text_evictions: self.text_evictions,
            largest_entry_bytes: self
                .entries
                .values()
                .map(|entry| entry.bytes)
                .max()
                .unwrap_or(0),
            largest_text_entry_bytes: self
                .paragraphs
                .values()
                .map(|entry| entry.bytes)
                .max()
                .unwrap_or(0),
        }
    }
}

fn scene_text_layout_request<'a>(
    text: &'a str,
    rect: UiRect,
    style: TextStyle,
) -> crate::text::TextLayoutRequest<'a> {
    let mut request =
        crate::text::TextLayoutRequest::single_line(text, rect, style.height, style.weight);
    request.tracking = style.tracking;
    request.align = style.align;
    request
}

fn paint_cached_paragraph(canvas: &Canvas, rect: UiRect, paragraph: &Paragraph) {
    let y = rect.top + ((rect.height() - paragraph.height()) * 0.5).max(0.0);
    canvas.save();
    canvas.clip_rect(sk_rect(rect), None, true);
    paragraph.paint(canvas, (rect.left, y));
    canvas.restore();
}

fn paragraph_cache_entry_bytes(text: &str, paragraph: &Paragraph) -> usize {
    2048usize
        .saturating_add(text.len())
        .saturating_add(paragraph.line_number().saturating_mul(256))
}

pub(crate) struct SkiaSoftwareSurface {
    pixels: Vec<u8>,
    size: (i32, i32),
    cache: SkiaCache,
}

impl SkiaSoftwareSurface {
    pub(crate) fn new(cache_budget: usize) -> Self {
        Self {
            pixels: Vec::new(),
            size: (0, 0),
            cache: SkiaCache::new(cache_budget),
        }
    }

    fn ensure_surface(&mut self, width: i32, height: i32) {
        let size = (width.max(1), height.max(1));
        if self.size != size {
            self.size = size;
            self.pixels.resize(size.0 as usize * size.1 as usize * 4, 0);
            self.cache.trim(MemoryPressure::Critical);
        }
    }

    pub(crate) fn draw(&mut self, scene: &Scene, frame: &FrameInfo<'_>) -> Result<(), String> {
        let viewport = frame.viewport();
        self.ensure_surface(viewport.width(), viewport.height());
        let info = ImageInfo::new(self.size, ColorType::BGRA8888, AlphaType::Premul, None);
        let mut surface = surfaces::wrap_pixels(
            &info,
            self.pixels.as_mut_slice(),
            self.size.0 as usize * 4,
            None,
        )
        .ok_or_else(|| "Skia could not wrap the retained software surface".to_owned())?;
        paint_scene_damage(surface.canvas(), &mut self.cache, scene, frame)
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn size(&self) -> (i32, i32) {
        self.size
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        self.cache.trim(pressure);
        if pressure == MemoryPressure::Critical {
            self.pixels.clear();
            self.size = (0, 0);
        }
    }

    pub(crate) fn cache_stats(&self) -> SkiaCacheStats {
        self.cache.stats()
    }
}

pub(crate) fn paint_scene_damage(
    canvas: &Canvas,
    cache: &mut SkiaCache,
    scene: &Scene,
    frame: &FrameInfo<'_>,
) -> Result<(), String> {
    cache.begin_frame();
    let clips: Vec<PhysicalRect> = if frame.is_full_redraw() {
        vec![frame.viewport()]
    } else {
        frame.damage().to_vec()
    };
    let mut painter = SkiaPainter { cache };
    for clip in clips {
        canvas.save();
        canvas.clip_rect(physical_rect(clip), None, false);
        canvas.clear(SkColor::TRANSPARENT);
        painter.draw_commands(canvas, scene.commands(), Some(ui_rect_from_physical(clip)))?;
        canvas.restore();
    }
    Ok(())
}

struct SkiaPainter<'a> {
    cache: &'a mut SkiaCache,
}

impl SkiaPainter<'_> {
    fn draw_commands(
        &mut self,
        canvas: &Canvas,
        commands: &[ScenePrimitive],
        clip: Option<UiRect>,
    ) -> Result<(), String> {
        for command in commands {
            if clip.is_some_and(|clip| clip.intersect(command.paint_bounds()).is_none()) {
                continue;
            }
            self.draw_command(canvas, command)?;
        }
        Ok(())
    }

    fn draw_command(&mut self, canvas: &Canvas, command: &ScenePrimitive) -> Result<(), String> {
        match command {
            ScenePrimitive::Rect { rect, style, .. } => draw_rect(canvas, *rect, *style),
            ScenePrimitive::Ellipse { rect, style, .. } => draw_ellipse(canvas, *rect, *style),
            ScenePrimitive::Text {
                rect, text, style, ..
            } => self.cache.draw_text(canvas, *rect, text, *style),
            ScenePrimitive::Line {
                start, end, stroke, ..
            } => {
                canvas.draw_line((start.x, start.y), (end.x, end.y), &stroke_paint(*stroke));
            }
            ScenePrimitive::Path { path, style, .. } => draw_path(canvas, path, *style),
            ScenePrimitive::Image {
                rect, source, fit, ..
            } => {
                if let Some(image) = self.image(source)? {
                    draw_image(canvas, &image, *rect, *fit, None);
                }
            }
            ScenePrimitive::Icon {
                rect, key, style, ..
            } => {
                if let Some(image) = self.icon(key, *rect, *style)? {
                    let destination = sk_rect(*rect);
                    canvas.draw_image_rect(image, None, &destination, &Paint::default());
                }
            }
            ScenePrimitive::Glow {
                rect, color, alpha, ..
            } => draw_glow(canvas, *rect, *color, *alpha),
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                self.draw_backdrop(canvas, *rect, None, *style)?
            }
            ScenePrimitive::BackdropBlurPath {
                rect, path, style, ..
            } => self.draw_backdrop(canvas, *rect, Some(path), *style)?,
            ScenePrimitive::Overlay { rect, style, .. } => draw_overlay(canvas, *rect, style),
            ScenePrimitive::Custom {
                rect, key, style, ..
            } => {
                if let Some(style) = style {
                    if let Some(fragment) = custom_scene(key, *rect, *style)? {
                        self.draw_commands(canvas, fragment.commands(), Some(*rect))?;
                    }
                }
            }
            ScenePrimitive::CompositingLayer {
                id,
                rect,
                spec,
                commands,
                content_signature,
                ..
            } => {
                let key = format!("composite:{}:{content_signature}", id.as_str());
                let image = if let Some(image) = self.cache.get(&key) {
                    image
                } else {
                    let image = self.render_layer(
                        (rect.width(), rect.height()),
                        commands,
                        0.0,
                        0.0,
                        spec.background == CompositingLayerBackground::Opaque,
                    )?;
                    self.cache.insert(key, image)
                };
                draw_composited(canvas, &image, *rect, spec.opacity, spec.transform);
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                let cacheable = spec.cache_policy != StaticLayerCachePolicy::Disabled;
                let key = format!(
                    "static:{}:{}:{}:{}x{}",
                    id.as_str(),
                    spec.revision,
                    child_signature,
                    rect.width().ceil(),
                    rect.height().ceil()
                );
                let image = if cacheable {
                    self.cache.get(&key)
                } else {
                    None
                };
                let image = match image {
                    Some(image) => image,
                    None => {
                        let mut surface = layer_surface(rect.width(), rect.height())?;
                        let layer_canvas = surface.canvas();
                        if spec.background == StaticLayerBackground::Opaque {
                            layer_canvas.clear(SkColor::BLACK);
                        } else {
                            layer_canvas.clear(SkColor::TRANSPARENT);
                        }
                        if let StaticLayerSource::BakedAsset { key, fit }
                        | StaticLayerSource::Hybrid {
                            baked_base: Some(key),
                            fit,
                        } = spec.source
                        {
                            if let Some(base) = self.image(&UiImageSource::Static(key))? {
                                draw_image(
                                    layer_canvas,
                                    &base,
                                    UiRect::new(0.0, 0.0, rect.width(), rect.height()),
                                    fit,
                                    None,
                                );
                            }
                        }
                        layer_canvas.save();
                        layer_canvas.translate((-rect.left, -rect.top));
                        self.draw_commands(layer_canvas, commands, Some(*rect))?;
                        layer_canvas.restore();
                        let image = surface.image_snapshot();
                        if cacheable {
                            self.cache.insert(key, image)
                        } else {
                            image
                        }
                    }
                };
                let destination = rect.translate(spec.offset_x, spec.offset_y);
                let mut paint = Paint::default();
                paint.set_alpha(spec.opacity);
                let destination = sk_rect(destination);
                canvas.draw_image_rect(image, None, &destination, &paint);
            }
            ScenePrimitive::ScrollRaster {
                id,
                viewport,
                spec,
                commands,
                child_signature,
                ..
            } => {
                self.draw_scroll_raster(canvas, id, *viewport, spec, commands, *child_signature)?
            }
            ScenePrimitive::Clip { rect, commands, .. } => {
                canvas.save();
                canvas.clip_rect(sk_rect(*rect), None, true);
                self.draw_commands(canvas, commands, Some(*rect))?;
                canvas.restore();
            }
            ScenePrimitive::ClipPath {
                rect,
                path,
                commands,
                ..
            } => {
                canvas.save();
                canvas.clip_path(&sk_path(path), None, true);
                self.draw_commands(canvas, commands, Some(*rect))?;
                canvas.restore();
            }
        }
        Ok(())
    }

    fn render_layer(
        &mut self,
        size: (f32, f32),
        commands: &[ScenePrimitive],
        offset_x: f32,
        offset_y: f32,
        opaque: bool,
    ) -> Result<Image, String> {
        let mut surface = layer_surface(size.0, size.1)?;
        let canvas = surface.canvas();
        canvas.clear(if opaque {
            SkColor::BLACK
        } else {
            SkColor::TRANSPARENT
        });
        canvas.translate((-offset_x, -offset_y));
        self.draw_commands(canvas, commands, None)?;
        Ok(surface.image_snapshot())
    }

    fn image(&mut self, source: &UiImageSource) -> Result<Option<Image>, String> {
        let key = image_key(source);
        if let Some(image) = self.cache.get(&key) {
            return Ok(Some(image));
        }
        let bytes: Arc<[u8]> = match source {
            UiImageSource::Static(id) => match render_resources().resolver() {
                Some(resolver) => resolver.resolve(id).map_err(|error| error.to_string())?,
                None => return Ok(None),
            },
            UiImageSource::File(_) | UiImageSource::Url(_) => {
                let Some(bytes) = crate::assets::cached_image_bytes(source) else {
                    let _ = crate::assets::request_image(source);
                    return Ok(None);
                };
                bytes
            }
            UiImageSource::Bytes { bytes, .. } => Arc::from(bytes.as_slice()),
        };
        let image = Image::from_encoded(Data::new_copy(bytes.as_ref()))
            .ok_or_else(|| format!("Skia could not decode image {key}"))?;
        Ok(Some(self.cache.insert(key, image)))
    }

    fn icon(
        &mut self,
        key: &'static str,
        rect: UiRect,
        style: crate::core::IconStyle,
    ) -> Result<Option<Image>, String> {
        let width = rect.width().ceil().max(1.0) as i32;
        let height = rect.height().ceil().max(1.0) as i32;
        let cache_key = format!(
            "icon:{key}:{width}x{height}:{}:{}",
            style.color.0, style.alpha
        );
        if let Some(image) = self.cache.get(&cache_key) {
            return Ok(Some(image));
        }
        let Some(svg) = crate::icons::resolve_svg(key) else {
            return Ok(None);
        };
        let tinted = tint_svg(&svg, style.color, style.alpha);
        let dom = skia_safe::svg::Dom::from_bytes(tinted.as_bytes(), FontMgr::default())
            .map_err(|_| format!("Skia could not parse SVG icon {key}"))?;
        let mut surface = layer_surface(width as f32, height as f32)?;
        surface.canvas().clear(SkColor::TRANSPARENT);
        let intrinsic = dom.root().intrinsic_size();
        if intrinsic.width > 0.0 && intrinsic.height > 0.0 {
            surface.canvas().scale((
                width as f32 / intrinsic.width,
                height as f32 / intrinsic.height,
            ));
        }
        dom.render(surface.canvas());
        Ok(Some(self.cache.insert(cache_key, surface.image_snapshot())))
    }

    fn draw_backdrop(
        &mut self,
        canvas: &Canvas,
        rect: UiRect,
        path: Option<&UiPath>,
        style: BackdropBlurStyle,
    ) -> Result<(), String> {
        let Some(image) = self.image(&UiImageSource::Static(style.source))? else {
            return Ok(());
        };
        canvas.save();
        if let Some(path) = path {
            canvas.clip_path(&sk_path(path), None, true);
        } else {
            canvas.clip_rect(sk_rect(rect), None, true);
        }
        let mut paint = Paint::default();
        paint.set_alpha_f(style.opacity.clamp(0.0, 1.0));
        paint.set_image_filter(skia_safe::image_filters::blur(
            (style.radius.max(0.0), style.radius.max(0.0)),
            TileMode::Clamp,
            None,
            None,
        ));
        draw_image(canvas, &image, style.source_rect, style.fit, Some(&paint));
        if style.tint_alpha > 0.0 {
            let mut tint = color_paint(style.tint, (style.tint_alpha * 255.0).round() as u8);
            tint.set_blend_mode(BlendMode::SrcOver);
            canvas.draw_rect(sk_rect(rect), &tint);
        }
        canvas.restore();
        Ok(())
    }

    fn draw_scroll_raster(
        &mut self,
        canvas: &Canvas,
        id: &crate::core::UiId,
        viewport: UiRect,
        spec: &crate::core::ScrollRasterSpec,
        commands: &[ScenePrimitive],
        child_signature: u64,
    ) -> Result<(), String> {
        let tile_height = spec.tile_height_px.max(1.0);
        let render_tile = |painter: &mut Self, tile_index: usize| -> Result<Image, String> {
            let tile_top = tile_index as f32 * tile_height;
            let height = (spec.content_height - tile_top).clamp(0.0, tile_height);
            let key = format!(
                "scroll:{}:{}:{}:{}:{}x{}",
                id.as_str(),
                spec.cache_epoch,
                child_signature,
                tile_index,
                viewport.width().ceil(),
                height.ceil()
            );
            if let Some(image) = painter.cache.get(&key) {
                return Ok(image);
            }
            let mut surface = layer_surface(viewport.width(), height.max(1.0))?;
            let tile_canvas = surface.canvas();
            if let Some(fill) = spec.background_fill {
                tile_canvas.clear(sk_color(fill, 255));
            } else {
                tile_canvas.clear(SkColor::TRANSPARENT);
            }
            tile_canvas.translate((-viewport.left, -(viewport.top + tile_top)));
            let content_clip = UiRect::new(
                viewport.left,
                viewport.top + tile_top,
                viewport.right,
                viewport.top + tile_top + height,
            );
            painter.draw_commands(tile_canvas, commands, Some(content_clip))?;
            Ok(painter.cache.insert(key, surface.image_snapshot()))
        };

        let prefetch_started = Instant::now();
        let prefetch_budget = Duration::from_millis(spec.max_prefetch_ms_per_frame as u64);
        for tile in spec
            .prefetch_tiles
            .iter()
            .copied()
            .take(spec.max_prefetch_tiles_per_frame)
        {
            if prefetch_budget.is_zero() || prefetch_started.elapsed() >= prefetch_budget {
                break;
            }
            let _ = render_tile(self, tile)?;
        }

        canvas.save();
        canvas.clip_rect(sk_rect(viewport), None, false);
        for tile in spec.visible_tiles.iter().copied() {
            let image = render_tile(self, tile)?;
            let tile_top = tile as f32 * tile_height;
            let top = viewport.top + tile_top - spec.scroll_y;
            let destination = Rect::new(
                viewport.left,
                top,
                viewport.right,
                top + image.height() as f32,
            );
            canvas.draw_image_rect(image, None, &destination, &Paint::default());
        }
        canvas.restore();
        Ok(())
    }
}

fn layer_surface(width: f32, height: f32) -> Result<Surface, String> {
    surfaces::raster_n32_premul((width.ceil().max(1.0) as i32, height.ceil().max(1.0) as i32))
        .ok_or_else(|| "Skia could not create an offscreen layer".to_owned())
}

fn draw_rect(canvas: &Canvas, rect: UiRect, style: VisualStyle) {
    let area = sk_rect(rect);
    if let Some(fill) = style.fill {
        let paint = color_paint(fill, style.fill_alpha);
        if style.radius > 0.0 {
            canvas.draw_rrect(RRect::new_rect_xy(area, style.radius, style.radius), &paint);
        } else {
            canvas.draw_rect(area, &paint);
        }
    }
    if let Some(stroke) = style.stroke {
        let paint = stroke_paint(stroke);
        if style.radius > 0.0 {
            canvas.draw_rrect(RRect::new_rect_xy(area, style.radius, style.radius), &paint);
        } else {
            canvas.draw_rect(area, &paint);
        }
    }
}

fn draw_ellipse(canvas: &Canvas, rect: UiRect, style: VisualStyle) {
    if let Some(fill) = style.fill {
        canvas.draw_oval(sk_rect(rect), &color_paint(fill, style.fill_alpha));
    }
    if let Some(stroke) = style.stroke {
        canvas.draw_oval(sk_rect(rect), &stroke_paint(stroke));
    }
}

fn draw_path(canvas: &Canvas, path: &UiPath, style: PathStyle) {
    let path = sk_path(path);
    if let Some(fill) = style.fill {
        canvas.draw_path(&path, &color_paint(fill, style.fill_alpha));
    }
    if let Some(stroke) = style.stroke {
        canvas.draw_path(&path, &stroke_paint(stroke));
    }
}

fn draw_image(canvas: &Canvas, image: &Image, rect: UiRect, fit: ImageFit, paint: Option<&Paint>) {
    let image_size = (image.width() as f32, image.height() as f32);
    let destination = fitted_rect(rect, image_size, fit);
    let default_paint = Paint::default();
    if fit == ImageFit::Cover {
        canvas.save();
        canvas.clip_rect(sk_rect(rect), None, true);
    }
    canvas.draw_image_rect_with_sampling_options(
        image,
        None,
        sk_rect(destination),
        SamplingOptions::default(),
        paint.unwrap_or(&default_paint),
    );
    if fit == ImageFit::Cover {
        canvas.restore();
    }
}

fn draw_glow(canvas: &Canvas, rect: UiRect, color: Color, alpha: u8) {
    let center = (
        rect.left + rect.width() / 2.0,
        rect.top + rect.height() / 2.0,
    );
    let colors = [
        sk_color_f(color, alpha as f32 / 255.0),
        sk_color_f(color, 0.0),
    ];
    let positions = [0.0, 1.0];
    let gradient = skia_safe::gradient::Gradient::new(
        skia_safe::gradient::Colors::new(
            colors.as_slice(),
            Some(positions.as_slice()),
            TileMode::Clamp,
            None,
        ),
        skia_safe::gradient::Interpolation::default(),
    );
    if let Some(shader) = skia_safe::gradient::shaders::radial_gradient(
        (center, rect.width().max(rect.height()) / 2.0),
        &gradient,
        None,
    ) {
        let mut paint = Paint::default();
        paint.set_shader(shader);
        canvas.draw_rect(sk_rect(rect), &paint);
    }
}

fn draw_overlay(canvas: &Canvas, rect: UiRect, style: &crate::core::OverlayStyle) {
    for layer in &style.vertical_layers {
        let colors = [
            sk_color_f(layer.color, layer.alpha_top),
            sk_color_f(layer.color, layer.alpha_bottom),
        ];
        let gradient = skia_safe::gradient::Gradient::new(
            skia_safe::gradient::Colors::new_evenly_spaced(
                colors.as_slice(),
                TileMode::Clamp,
                None,
            ),
            skia_safe::gradient::Interpolation::default(),
        );
        if let Some(shader) = skia_safe::gradient::shaders::linear_gradient(
            ((rect.left, rect.top), (rect.left, rect.bottom)),
            &gradient,
            None,
        ) {
            let mut paint = Paint::default();
            paint.set_shader(shader);
            canvas.draw_rect(sk_rect(rect), &paint);
        }
    }
    for layer in &style.radial_layers {
        let center = (
            rect.left + rect.width() * layer.center_x,
            rect.top + rect.height() * layer.center_y,
        );
        let radius = rect.width().min(rect.height()) * layer.radius;
        let colors = [
            sk_color_f(layer.color, layer.alpha),
            sk_color_f(layer.color, 0.0),
        ];
        let gradient = skia_safe::gradient::Gradient::new(
            skia_safe::gradient::Colors::new_evenly_spaced(
                colors.as_slice(),
                TileMode::Clamp,
                None,
            ),
            skia_safe::gradient::Interpolation::default(),
        );
        if let Some(shader) =
            skia_safe::gradient::shaders::radial_gradient((center, radius), &gradient, None)
        {
            let mut paint = Paint::default();
            paint.set_shader(shader);
            canvas.draw_rect(sk_rect(rect), &paint);
        }
    }
}

fn draw_composited(
    canvas: &Canvas,
    image: &Image,
    rect: UiRect,
    opacity: u8,
    transform: LayerTransform,
) {
    canvas.save();
    let origin = (
        rect.left + rect.width() * transform.origin_x(),
        rect.top + rect.height() * transform.origin_y(),
    );
    canvas.translate((
        origin.0 + transform.translation_x(),
        origin.1 + transform.translation_y(),
    ));
    canvas.rotate(transform.rotation_degrees_f32(), None);
    canvas.scale((transform.scale_x(), transform.scale_y()));
    canvas.translate((-origin.0, -origin.1));
    let mut paint = Paint::default();
    paint.set_alpha(opacity);
    let destination = sk_rect(rect);
    canvas.draw_image_rect(image, None, &destination, &paint);
    canvas.restore();
}

fn custom_scene(
    key: &str,
    rect: UiRect,
    style: crate::core::CustomPaintStyle,
) -> Result<Option<crate::assets::SceneFragment>, String> {
    let Some(provider) = render_resources().custom_paint().cloned() else {
        return Ok(None);
    };
    provider
        .record(key, rect, style)
        .map_err(|error| error.to_string())
}

fn fitted_rect(bounds: UiRect, image: (f32, f32), fit: ImageFit) -> UiRect {
    if fit == ImageFit::Fill || image.0 <= 0.0 || image.1 <= 0.0 {
        return bounds;
    }
    let scale = match fit {
        ImageFit::Contain => (bounds.width() / image.0).min(bounds.height() / image.1),
        ImageFit::Cover => (bounds.width() / image.0).max(bounds.height() / image.1),
        ImageFit::Fill => 1.0,
    };
    let width = image.0 * scale;
    let height = image.1 * scale;
    UiRect::new(
        bounds.left + (bounds.width() - width) / 2.0,
        bounds.top + (bounds.height() - height) / 2.0,
        bounds.left + (bounds.width() + width) / 2.0,
        bounds.top + (bounds.height() + height) / 2.0,
    )
}

fn sk_path(path: &UiPath) -> Path {
    let mut result = PathBuilder::new();
    for command in path.commands() {
        match command {
            UiPathCommand::MoveTo(point) => {
                result.move_to((point.x, point.y));
            }
            UiPathCommand::LineTo(point) => {
                result.line_to((point.x, point.y));
            }
            UiPathCommand::QuadraticTo { control, to } => {
                result.quad_to((control.x, control.y), (to.x, to.y));
            }
            UiPathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                result.cubic_to(
                    (control1.x, control1.y),
                    (control2.x, control2.y),
                    (to.x, to.y),
                );
            }
            UiPathCommand::Close => {
                result.close();
            }
        }
    }
    result.detach()
}

fn stroke_paint(stroke: Stroke) -> Paint {
    let mut paint = color_paint(stroke.color, stroke.alpha);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(stroke.width.max(0.0));
    paint
}

fn color_paint(color: Color, alpha: u8) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(sk_color(color, alpha));
    paint
}

fn sk_color(color: Color, alpha: u8) -> SkColor {
    SkColor::from_argb(
        alpha,
        ((color.0 >> 16) & 0xFF) as u8,
        ((color.0 >> 8) & 0xFF) as u8,
        (color.0 & 0xFF) as u8,
    )
}

fn sk_color_f(color: Color, alpha: f32) -> Color4f {
    Color4f::new(
        ((color.0 >> 16) & 0xFF) as f32 / 255.0,
        ((color.0 >> 8) & 0xFF) as f32 / 255.0,
        (color.0 & 0xFF) as f32 / 255.0,
        alpha.clamp(0.0, 1.0),
    )
}

fn sk_rect(rect: UiRect) -> Rect {
    Rect::new(rect.left, rect.top, rect.right, rect.bottom)
}

fn physical_rect(rect: PhysicalRect) -> Rect {
    Rect::new(
        rect.left as f32,
        rect.top as f32,
        rect.right as f32,
        rect.bottom as f32,
    )
}

fn ui_rect_from_physical(rect: PhysicalRect) -> UiRect {
    UiRect::new(
        rect.left as f32,
        rect.top as f32,
        rect.right as f32,
        rect.bottom as f32,
    )
}

fn image_key(source: &UiImageSource) -> String {
    match source {
        UiImageSource::Static(key) => format!("asset:{key}"),
        UiImageSource::File(path) => format!("file:{}", path.display()),
        UiImageSource::Url(url) => format!("url:{url}"),
        UiImageSource::Bytes { key, version, .. } => format!("bytes:{key}:{version}"),
    }
}

fn tint_svg(svg: &str, color: Color, alpha: u8) -> String {
    let hex = format!("#{:06X}", color.0 & 0xFFFFFF);
    svg.replace("currentColor", &hex)
        .replace("currentOpacity", &format!("{:.6}", alpha as f32 / 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assets::{
            AssetBytes, AssetError, AssetResolver, CustomPaintProvider, RenderResources,
            SceneFragment,
        },
        core::{
            CompositingLayerSpec, CustomPaintStyle, IconStyle, OverlayStyle, Point,
            RadialGradientLayer, RenderPhase, ScenePrimitiveKind, ScrollRasterSpec,
            StaticLayerSource, StaticLayerSpec, UiId, UiPathCommand, UiScale,
            VerticalGradientLayer,
        },
        renderer::FrameReason,
    };

    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5,
        0x1C, 0x0C, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC,
        0xFF, 0x1F, 0x00, 0x02, 0xEB, 0x01, 0xF5, 0x8F, 0x59, 0x97, 0xDB, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    struct TestAssets;

    impl AssetResolver for TestAssets {
        fn resolve(&self, id: &str) -> Result<AssetBytes, AssetError> {
            (id == "test.pixel")
                .then(|| Arc::<[u8]>::from(PIXEL_PNG))
                .ok_or_else(|| AssetError::NotFound(id.to_owned()))
        }
    }

    struct TestCustomPaint;

    impl CustomPaintProvider for TestCustomPaint {
        fn record(
            &self,
            key: &str,
            bounds: UiRect,
            _style: CustomPaintStyle,
        ) -> Result<Option<SceneFragment>, AssetError> {
            Ok((key == "test.custom").then(|| {
                SceneFragment::new(vec![ScenePrimitive::Rect {
                    id: test_id("custom.fragment"),
                    rect: bounds,
                    style: VisualStyle::filled(Color(0x44CC88)),
                    phase: RenderPhase::Content,
                }])
            }))
        }
    }

    fn test_id(name: &str) -> UiId {
        UiId::from_parts(["skia-test", name])
    }

    fn rect_command(name: &str, rect: UiRect, color: Color) -> ScenePrimitive {
        ScenePrimitive::Rect {
            id: test_id(name),
            rect,
            style: VisualStyle::filled(color),
            phase: RenderPhase::Content,
        }
    }

    fn triangle(rect: UiRect) -> UiPath {
        UiPath::new([
            UiPathCommand::MoveTo(Point::new(rect.left, rect.bottom)),
            UiPathCommand::LineTo(Point::new(rect.left + rect.width() / 2.0, rect.top)),
            UiPathCommand::LineTo(Point::new(rect.right, rect.bottom)),
            UiPathCommand::Close,
        ])
    }

    fn draw_scene(
        surface: &mut SkiaSoftwareSurface,
        scene: &Scene,
        full: bool,
        damage: &[PhysicalRect],
    ) {
        let frame = FrameInfo::new(
            PhysicalRect::new(0, 0, 32, 32),
            damage,
            UiScale::ONE,
            FrameReason::SceneChange,
            full,
        );
        let resources = RenderResources::new()
            .with_resolver(TestAssets)
            .with_custom_paint(TestCustomPaint);
        crate::assets::with_render_resources(resources, || {
            surface.draw(scene, &frame).expect("draw conformance scene")
        });
    }

    fn pixel(surface: &SkiaSoftwareSurface, x: usize, y: usize) -> [u8; 4] {
        let (width, _) = surface.size();
        let offset = (y * width as usize + x) * 4;
        surface.pixels()[offset..offset + 4].try_into().unwrap()
    }

    fn primitive_inventory() -> Vec<ScenePrimitive> {
        let rect = UiRect::new(4.0, 4.0, 20.0, 20.0);
        let child = vec![rect_command("child", rect, Color(0x33AAEE))];
        vec![
            rect_command("rect", rect, Color(0xFF0000)),
            ScenePrimitive::Ellipse {
                id: test_id("ellipse"),
                rect,
                style: VisualStyle::filled(Color(0x00FF00)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Text {
                id: test_id("text"),
                rect,
                text: "Skia".into(),
                style: TextStyle::new(Color::WHITE, 12.0, 400),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Custom {
                id: test_id("custom"),
                rect,
                key: "test.custom",
                style: Some(CustomPaintStyle::new(Color::WHITE, 1.0)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Line {
                id: test_id("line"),
                start: Point::new(4.0, 4.0),
                end: Point::new(20.0, 20.0),
                stroke: Stroke::new(Color::WHITE, 2.0, 255),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Path {
                id: test_id("path"),
                rect,
                path: triangle(rect),
                style: PathStyle::filled(Color(0xFFCC00)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Image {
                id: test_id("image"),
                rect,
                source: UiImageSource::bytes("test.pixel", 1, Arc::new(PIXEL_PNG.to_vec())),
                fit: ImageFit::Fill,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Icon {
                id: test_id("icon"),
                rect,
                key: "copy",
                style: IconStyle::new(Color::WHITE),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Glow {
                id: test_id("glow"),
                rect,
                color: Color(0x33AAFF),
                alpha: 180,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::BackdropBlur {
                id: test_id("blur"),
                rect,
                style: BackdropBlurStyle::new("test.pixel", ImageFit::Fill, rect).radius(2.0),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::BackdropBlurPath {
                id: test_id("blur-path"),
                rect,
                path: triangle(rect),
                style: BackdropBlurStyle::new("test.pixel", ImageFit::Fill, rect).radius(2.0),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Overlay {
                id: test_id("overlay"),
                rect,
                style: OverlayStyle::new()
                    .vertical(VerticalGradientLayer::new(Color::WHITE, 0.8, 0.1))
                    .radial(RadialGradientLayer::new(
                        Color(0x44AAFF),
                        0.7,
                        0.5,
                        0.5,
                        0.5,
                    )),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::CompositingLayer {
                id: test_id("compositing"),
                rect,
                spec: CompositingLayerSpec::new().opacity(0.75),
                commands: child.clone(),
                content_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::StaticLayer {
                id: test_id("static"),
                rect,
                spec: StaticLayerSpec::new(StaticLayerSource::runtime()).transparent_background(),
                commands: child.clone(),
                child_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::ScrollRaster {
                id: test_id("scroll"),
                viewport: rect,
                spec: ScrollRasterSpec {
                    cache_epoch: 1,
                    content_height: rect.height(),
                    scroll_y: 0.0,
                    tile_height_px: rect.height(),
                    memory_budget_bytes: 1024 * 1024,
                    background_fill: None,
                    visible_tiles: vec![0],
                    prefetch_tiles: Vec::new(),
                    max_prefetch_tiles_per_frame: 0,
                    max_prefetch_ms_per_frame: 0,
                },
                commands: child.clone(),
                child_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Clip {
                id: test_id("clip"),
                rect,
                commands: child.clone(),
                child_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::ClipPath {
                id: test_id("clip-path"),
                rect,
                path: triangle(rect),
                commands: child,
                child_signature: 1,
                phase: RenderPhase::Content,
            },
        ]
    }

    #[test]
    fn software_probe_creates_a_real_skia_surface() {
        assert!(probe_skia_support(GraphicsPreference::Software).is_ok());
    }

    #[test]
    fn gpu_probe_reports_only_compiled_drivers() {
        assert_eq!(
            probe_skia_support(GraphicsPreference::OpenGl).is_ok(),
            cfg!(feature = "renderer-skia-gl")
        );
        #[cfg(all(
            feature = "renderer-skia-vulkan",
            any(target_os = "windows", target_os = "linux")
        ))]
        assert_eq!(
            probe_skia_support(GraphicsPreference::Vulkan).is_ok(),
            unsafe { ash::Entry::load() }.is_ok()
        );
        #[cfg(not(all(
            feature = "renderer-skia-vulkan",
            any(target_os = "windows", target_os = "linux")
        )))]
        assert!(probe_skia_support(GraphicsPreference::Vulkan).is_err());
        assert_eq!(
            probe_skia_support(GraphicsPreference::Metal).is_ok(),
            cfg!(all(feature = "renderer-skia-metal", target_os = "macos"))
        );
    }

    #[test]
    fn cache_is_byte_bounded() {
        let mut cache = SkiaCache::new(4 * 4 * 4);
        cache.begin_frame();
        let first = layer_surface(4.0, 4.0).unwrap().image_snapshot();
        cache.insert("first".to_owned(), first);
        cache.begin_frame();
        let second = layer_surface(4.0, 4.0).unwrap().image_snapshot();
        cache.insert("second".to_owned(), second);
        assert!(cache.resident_bytes <= cache.budget_bytes);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn image_fit_preserves_aspect_ratio() {
        assert_eq!(
            fitted_rect(
                UiRect::new(0.0, 0.0, 100.0, 100.0),
                (200.0, 100.0),
                ImageFit::Contain
            ),
            UiRect::new(0.0, 25.0, 100.0, 75.0)
        );
    }

    #[test]
    fn every_scene_primitive_has_a_real_skia_paint_path() {
        let commands = primitive_inventory();
        let kinds = commands
            .iter()
            .map(ScenePrimitive::kind)
            .collect::<Vec<_>>();
        assert_eq!(kinds, ScenePrimitiveKind::ALL);
        for command in commands {
            let mut scene = Scene::new();
            scene.push(command);
            let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
            draw_scene(
                &mut surface,
                &scene,
                true,
                &[PhysicalRect::new(0, 0, 32, 32)],
            );
        }
    }

    #[test]
    fn dirty_draw_preserves_pixels_outside_damage_and_clears_removals() {
        let full = [PhysicalRect::new(0, 0, 32, 32)];
        let left = [PhysicalRect::new(0, 0, 16, 32)];
        let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
        let mut red = Scene::new();
        red.push(rect_command(
            "background-red",
            UiRect::new(0.0, 0.0, 32.0, 32.0),
            Color(0xFF0000),
        ));
        draw_scene(&mut surface, &red, true, &full);
        let red_pixel = pixel(&surface, 24, 16);

        let mut blue = Scene::new();
        blue.push(rect_command(
            "background-blue",
            UiRect::new(0.0, 0.0, 32.0, 32.0),
            Color(0x0000FF),
        ));
        draw_scene(&mut surface, &blue, false, &left);
        assert_ne!(pixel(&surface, 8, 16), red_pixel);
        assert_eq!(pixel(&surface, 24, 16), red_pixel);

        draw_scene(&mut surface, &Scene::new(), false, &left);
        assert_eq!(pixel(&surface, 8, 16), [0, 0, 0, 0]);
        assert_eq!(pixel(&surface, 24, 16), red_pixel);
    }

    #[test]
    fn dpi_projection_and_nested_clip_use_physical_bounds() {
        let mut logical = Scene::new();
        logical.push(ScenePrimitive::Clip {
            id: test_id("outer-clip"),
            rect: UiRect::new(0.0, 0.0, 5.0, 5.0),
            commands: vec![ScenePrimitive::Clip {
                id: test_id("inner-clip"),
                rect: UiRect::new(2.0, 2.0, 5.0, 5.0),
                commands: vec![rect_command(
                    "clip-fill",
                    UiRect::new(0.0, 0.0, 8.0, 8.0),
                    Color::WHITE,
                )],
                child_signature: 1,
                phase: RenderPhase::Content,
            }],
            child_signature: 1,
            phase: RenderPhase::Content,
        });
        let scene = logical.project_to_physical(UiScale::new(2.0));
        let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
        draw_scene(
            &mut surface,
            &scene,
            true,
            &[PhysicalRect::new(0, 0, 32, 32)],
        );
        assert_eq!(pixel(&surface, 2, 2), [0, 0, 0, 0]);
        assert_ne!(pixel(&surface, 6, 6), [0, 0, 0, 0]);
        assert_eq!(pixel(&surface, 12, 12), [0, 0, 0, 0]);
    }

    #[test]
    fn cache_trim_is_deterministic_at_both_pressure_levels() {
        let mut cache = SkiaCache::new(1024 * 1024);
        cache.begin_frame();
        cache.insert(
            "old".to_owned(),
            layer_surface(4.0, 4.0).unwrap().image_snapshot(),
        );
        for _ in 0..4 {
            cache.begin_frame();
        }
        cache.insert(
            "recent".to_owned(),
            layer_surface(4.0, 4.0).unwrap().image_snapshot(),
        );
        cache.trim(MemoryPressure::Moderate);
        assert!(!cache.entries.contains_key("old"));
        assert!(cache.entries.contains_key("recent"));
        cache.trim(MemoryPressure::Critical);
        assert!(cache.entries.is_empty());
        assert_eq!(cache.resident_bytes, 0);
    }

    #[test]
    fn paragraph_layout_exposes_bidi_carets_selection_and_hit_testing() {
        let request = crate::text::TextLayoutRequest::single_line(
            "abc \u{05d0}\u{05d1}\u{05d2}",
            UiRect::new(0.0, 0.0, 240.0, 32.0),
            -16.0,
            400,
        );
        let layout = crate::text::TextSystem::layout(&SkiaTextSystem, &request).unwrap();
        assert!(layout.width > 0.0);
        assert!(layout.caret_rect(0).is_some());
        assert!(layout.caret_rect(request.text.chars().count()).is_some());
        assert!(layout
            .clusters()
            .iter()
            .any(|cluster| cluster.direction == crate::text::TextDirection::LeftToRight));
        assert!(layout
            .clusters()
            .iter()
            .any(|cluster| cluster.direction == crate::text::TextDirection::RightToLeft));
        assert!(!layout.selection_rects(1..6).is_empty());
        let cluster = layout.clusters().last().unwrap();
        let hit = layout.hit_test(
            (cluster.bounds.left + cluster.bounds.right) * 0.5,
            (cluster.bounds.top + cluster.bounds.bottom) * 0.5,
        );
        assert!(hit.index <= request.text.chars().count());
        assert!(hit.inside);
    }

    #[test]
    fn paragraph_clusters_keep_combining_sequences_together() {
        let request = crate::text::TextLayoutRequest::single_line(
            "a\u{0301}b",
            UiRect::new(0.0, 0.0, 120.0, 32.0),
            -16.0,
            400,
        );
        let layout = crate::text::TextSystem::layout(&SkiaTextSystem, &request).unwrap();
        assert!(layout
            .clusters()
            .iter()
            .any(|cluster| cluster.range == (0..2)));
    }

    #[test]
    fn paragraph_cache_is_reused_reported_and_trimmed() {
        let mut cache = SkiaCache::new(1024 * 1024);
        let mut surface = layer_surface(200.0, 40.0).unwrap();
        let rect = UiRect::new(0.0, 0.0, 200.0, 40.0);
        let style = TextStyle::new(Color::WHITE, -16.0, 400);
        cache.begin_frame();
        cache.draw_text(surface.canvas(), rect, "cached paragraph", style);
        cache.begin_frame();
        cache.draw_text(surface.canvas(), rect, "cached paragraph", style);
        let stats = cache.stats();
        assert_eq!(stats.text_entries, 1);
        assert_eq!(stats.text_misses, 1);
        assert_eq!(stats.text_hits, 1);
        assert!(stats.text_resident_bytes > 0);
        assert!(stats.largest_text_entry_bytes > 0);
        cache.trim(MemoryPressure::Critical);
        assert!(cache.paragraphs.is_empty());
        assert_eq!(cache.resident_bytes, 0);
    }

    #[test]
    fn layer_opacity_and_content_signature_invalidate_retained_images() {
        let bounds = UiRect::new(0.0, 0.0, 16.0, 16.0);
        let layer = |signature, color| ScenePrimitive::CompositingLayer {
            id: test_id("retained-layer"),
            rect: bounds,
            spec: CompositingLayerSpec::new().opacity(0.5),
            commands: vec![rect_command("retained-child", bounds, color)],
            content_signature: signature,
            phase: RenderPhase::Content,
        };
        let damage = [PhysicalRect::new(0, 0, 32, 32)];
        let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
        let mut first = Scene::new();
        first.push(layer(1, Color(0xFF0000)));
        draw_scene(&mut surface, &first, true, &damage);
        let red = pixel(&surface, 8, 8);
        assert!(red[3] >= 126 && red[3] <= 129);

        let mut second = Scene::new();
        second.push(layer(2, Color(0x0000FF)));
        draw_scene(&mut surface, &second, true, &damage);
        let blue = pixel(&surface, 8, 8);
        assert_ne!(blue, red);
        assert!(surface.cache_stats().misses >= 2);
    }
}
