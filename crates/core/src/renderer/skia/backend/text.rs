use super::*;

pub(super) struct SkiaTextSystem;

thread_local! {
    pub(super) static SKIA_TEXT_FONTS: RefCell<FontCollection> = RefCell::new(skia_font_collection());
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

pub(super) fn skia_font_collection() -> FontCollection {
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

pub(super) fn build_skia_paragraph(
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
pub(super) fn skia_paragraph_text_style(
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

pub(super) fn resolved_text_direction(
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

pub(super) fn portable_text_layout(
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

pub(super) fn paragraph_caret_rect(
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

pub(super) fn offset_skia_rect(rect: Rect, offset_x: f32, offset_y: f32) -> UiRect {
    UiRect::new(
        rect.left + offset_x,
        rect.top + offset_y,
        rect.right + offset_x,
        rect.bottom + offset_y,
    )
}

pub(super) fn char_byte_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

pub(super) fn char_utf16_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(text.chars().count() + 1);
    let mut offset = 0;
    boundaries.push(offset);
    for ch in text.chars() {
        offset += ch.len_utf16();
        boundaries.push(offset);
    }
    boundaries
}

pub(super) fn byte_range_to_char_range(
    boundaries: &[usize],
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    byte_to_char_index(boundaries, range.start)..byte_to_char_index(boundaries, range.end)
}

pub(super) fn byte_to_char_index(boundaries: &[usize], byte: usize) -> usize {
    boundaries.partition_point(|boundary| *boundary < byte)
}

pub(super) fn utf16_range_to_char_range(
    boundaries: &[usize],
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    utf16_to_char_index(boundaries, range.start)..utf16_to_char_index(boundaries, range.end)
}

pub(super) fn utf16_to_char_index(boundaries: &[usize], offset: usize) -> usize {
    boundaries.partition_point(|boundary| *boundary < offset)
}
