//! Platform-neutral text layout, measurement, and interaction geometry.

use std::{cell::RefCell, ops::Range, sync::Arc};

use crate::core::{TextAlign, UiRect};

#[derive(Clone, Copy)]
pub(crate) struct FontFamilies(pub &'static [&'static str]);

thread_local! {
    static FONT_FAMILIES: RefCell<Vec<&'static [&'static str]>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct FontFamiliesGuard;

impl Drop for FontFamiliesGuard {
    fn drop(&mut self) {
        FONT_FAMILIES.with(|current| {
            current.borrow_mut().pop();
        });
    }
}

pub(crate) fn install_font_families(families: &'static [&'static str]) -> FontFamiliesGuard {
    FONT_FAMILIES.with(|current| current.borrow_mut().push(families));
    FontFamiliesGuard
}

pub(crate) fn font_families() -> &'static [&'static str] {
    FONT_FAMILIES.with(|current| current.borrow().last().copied().unwrap_or(&["Segoe UI"]))
}

#[derive(Clone, Debug)]
pub struct FontAsset {
    pub bytes: Arc<Vec<u8>>,
    pub family_alias: Option<String>,
}

impl FontAsset {
    pub fn new(bytes: impl Into<Arc<Vec<u8>>>) -> Self {
        Self {
            bytes: bytes.into(),
            family_alias: None,
        }
    }

    pub fn family_alias(mut self, alias: impl Into<String>) -> Self {
        self.family_alias = Some(alias.into());
        self
    }
}

#[derive(Clone, Default)]
pub(crate) struct FontAssets(pub Arc<Vec<FontAsset>>);

thread_local! {
    static FONT_ASSETS: RefCell<Vec<Arc<Vec<FontAsset>>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct FontAssetsGuard;

impl Drop for FontAssetsGuard {
    fn drop(&mut self) {
        FONT_ASSETS.with(|current| {
            current.borrow_mut().pop();
        });
    }
}

pub(crate) fn install_font_assets(assets: Arc<Vec<FontAsset>>) -> FontAssetsGuard {
    FONT_ASSETS.with(|current| current.borrow_mut().push(assets));
    FontAssetsGuard
}

pub(crate) fn font_assets() -> Arc<Vec<FontAsset>> {
    FONT_ASSETS.with(|current| current.borrow().last().cloned().unwrap_or_default())
}

#[derive(Clone, Copy, Debug)]
pub struct TextMeasureRequest<'a> {
    pub text: &'a str,
    pub bounds: UiRect,
    pub font_height: f32,
    pub font_weight: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAffinity {
    Upstream,
    #[default]
    Downstream,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextFontSlant {
    #[default]
    Upright,
    Italic,
    Oblique,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextFontWidth(pub i32);

impl Default for TextFontWidth {
    fn default() -> Self {
        Self(5)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextVerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextFeature<'a> {
    pub name: &'a str,
    pub value: i32,
}

#[derive(Clone, Debug, Default)]
pub struct TextSpan<'a> {
    pub range: Range<usize>,
    pub font_families: &'a [&'a str],
    pub font_height: Option<f32>,
    pub font_weight: Option<i32>,
    pub font_width: Option<TextFontWidth>,
    pub font_slant: Option<TextFontSlant>,
    pub locale: Option<&'a str>,
    pub tracking: Option<f32>,
    pub features: &'a [TextFeature<'a>],
}

#[derive(Clone, Debug)]
pub struct TextLayoutRequest<'a> {
    pub text: &'a str,
    pub bounds: UiRect,
    pub font_height: f32,
    pub font_weight: i32,
    pub font_width: TextFontWidth,
    pub font_slant: TextFontSlant,
    pub font_families: &'a [&'a str],
    pub locale: &'a str,
    pub tracking: f32,
    pub align: TextAlign,
    pub direction: TextDirection,
    pub max_lines: Option<usize>,
    pub line_height: Option<f32>,
    pub vertical_align: TextVerticalAlign,
    pub features: &'a [TextFeature<'a>],
    pub spans: &'a [TextSpan<'a>],
}

impl<'a> TextLayoutRequest<'a> {
    pub fn single_line(text: &'a str, bounds: UiRect, font_height: f32, font_weight: i32) -> Self {
        Self {
            text,
            bounds,
            font_height,
            font_weight,
            font_width: TextFontWidth::default(),
            font_slant: TextFontSlant::Upright,
            font_families: &[],
            locale: "",
            tracking: 0.0,
            align: TextAlign::Left,
            direction: TextDirection::Auto,
            max_lines: Some(1),
            line_height: None,
            vertical_align: TextVerticalAlign::Center,
            features: &[],
            spans: &[],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLineMetrics {
    pub range: Range<usize>,
    pub bounds: UiRect,
    pub baseline: f32,
    pub hard_break: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextCluster {
    pub range: Range<usize>,
    pub bounds: UiRect,
    pub direction: TextDirection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextHit {
    pub index: usize,
    pub affinity: TextAffinity,
    pub inside: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TextCaret {
    index: usize,
    affinity: TextAffinity,
    rect: UiRect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    pub width: f32,
    pub height: f32,
    pub did_exceed_max_lines: bool,
    lines: Vec<TextLineMetrics>,
    clusters: Vec<TextCluster>,
    carets: Vec<TextCaret>,
}

impl TextLayout {
    pub fn new(
        width: f32,
        height: f32,
        did_exceed_max_lines: bool,
        lines: Vec<TextLineMetrics>,
        clusters: Vec<TextCluster>,
        carets: Vec<(usize, TextAffinity, UiRect)>,
    ) -> Self {
        Self {
            width,
            height,
            did_exceed_max_lines,
            lines,
            clusters,
            carets: carets
                .into_iter()
                .map(|(index, affinity, rect)| TextCaret {
                    index,
                    affinity,
                    rect,
                })
                .collect(),
        }
    }

    pub fn lines(&self) -> &[TextLineMetrics] {
        &self.lines
    }

    pub fn clusters(&self) -> &[TextCluster] {
        &self.clusters
    }

    pub fn caret_rect(&self, index: usize) -> Option<UiRect> {
        self.caret_rect_with_affinity(index, TextAffinity::Downstream)
            .or_else(|| {
                self.carets
                    .iter()
                    .find(|caret| caret.index == index)
                    .map(|c| c.rect)
            })
    }

    pub fn caret_rect_with_affinity(&self, index: usize, affinity: TextAffinity) -> Option<UiRect> {
        self.carets
            .iter()
            .find(|caret| caret.index == index && caret.affinity == affinity)
            .map(|caret| caret.rect)
    }

    pub fn selection_rects(&self, range: Range<usize>) -> Vec<UiRect> {
        if range.start >= range.end {
            return Vec::new();
        }
        let mut rects = Vec::<UiRect>::new();
        for cluster in self
            .clusters
            .iter()
            .filter(|cluster| cluster.range.start < range.end && cluster.range.end > range.start)
        {
            if let Some(last) = rects.last_mut() {
                let same_line = (last.top - cluster.bounds.top).abs() < 0.5
                    && (last.bottom - cluster.bounds.bottom).abs() < 0.5;
                let adjacent = (last.right - cluster.bounds.left).abs() < 1.0
                    || (cluster.bounds.right - last.left).abs() < 1.0;
                if same_line && adjacent {
                    last.left = last.left.min(cluster.bounds.left);
                    last.right = last.right.max(cluster.bounds.right);
                    continue;
                }
            }
            rects.push(cluster.bounds);
        }
        rects
    }

    pub fn previous_cluster_boundary(&self, index: usize) -> usize {
        self.clusters
            .iter()
            .flat_map(|cluster| [cluster.range.start, cluster.range.end])
            .chain(
                self.lines
                    .iter()
                    .flat_map(|line| [line.range.start, line.range.end]),
            )
            .filter(|boundary| *boundary < index)
            .max()
            .unwrap_or(0)
    }

    pub fn next_cluster_boundary(&self, index: usize) -> usize {
        self.clusters
            .iter()
            .flat_map(|cluster| [cluster.range.start, cluster.range.end])
            .chain(
                self.lines
                    .iter()
                    .flat_map(|line| [line.range.start, line.range.end]),
            )
            .filter(|boundary| *boundary > index)
            .min()
            .unwrap_or(index)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> TextHit {
        let Some(line) =
            self.lines.iter().min_by(|left, right| {
                distance_to_axis(y, left.bounds.top, left.bounds.bottom)
                    .total_cmp(&distance_to_axis(y, right.bounds.top, right.bounds.bottom))
            })
        else {
            return TextHit::default();
        };
        let mut clusters = self
            .clusters
            .iter()
            .filter(|cluster| {
                cluster.range.start >= line.range.start && cluster.range.end <= line.range.end
            })
            .peekable();
        let Some(first) = clusters.peek().cloned() else {
            return TextHit {
                index: line.range.start,
                affinity: TextAffinity::Downstream,
                inside: rect_contains(line.bounds, x, y),
            };
        };
        let mut nearest = first;
        let mut nearest_distance = distance_to_axis(x, first.bounds.left, first.bounds.right);
        for cluster in clusters {
            let distance = distance_to_axis(x, cluster.bounds.left, cluster.bounds.right);
            if distance < nearest_distance {
                nearest = cluster;
                nearest_distance = distance;
            }
        }
        let midpoint = (nearest.bounds.left + nearest.bounds.right) * 0.5;
        let leading_half = x < midpoint;
        let index = match nearest.direction {
            TextDirection::RightToLeft if leading_half => nearest.range.end,
            TextDirection::RightToLeft => nearest.range.start,
            _ if leading_half => nearest.range.start,
            _ => nearest.range.end,
        };
        TextHit {
            index,
            affinity: if leading_half {
                TextAffinity::Downstream
            } else {
                TextAffinity::Upstream
            },
            inside: rect_contains(line.bounds, x, y),
        }
    }
}

fn distance_to_axis(value: f32, start: f32, end: f32) -> f32 {
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        0.0
    }
}

fn rect_contains(rect: UiRect, x: f32, y: f32) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

pub trait TextSystem: Send + Sync + 'static {
    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics>;

    fn layout(&self, _request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        None
    }
}

#[derive(Clone)]
pub struct TextSystemHandle(Arc<dyn TextSystem>);

impl TextSystemHandle {
    pub fn new(system: impl TextSystem) -> Self {
        Self(Arc::new(system))
    }

    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
        self.0.measure(request)
    }

    fn layout(&self, request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        self.0.layout(request)
    }
}

thread_local! {
    static TEXT_SYSTEM: RefCell<Option<TextSystemHandle>> = const { RefCell::new(None) };
}

pub(crate) struct TextSystemGuard {
    previous: Option<TextSystemHandle>,
}

impl Drop for TextSystemGuard {
    fn drop(&mut self) {
        TEXT_SYSTEM.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

pub(crate) fn install_text_system(system: TextSystemHandle) -> TextSystemGuard {
    let previous = TEXT_SYSTEM.with(|current| current.borrow_mut().replace(system));
    TextSystemGuard { previous }
}

pub fn measure(request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
    TEXT_SYSTEM.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|system| system.measure(request))
    })
}

pub fn layout(request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
    TEXT_SYSTEM.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|system| system.layout(request))
    })
}

pub fn measure_width(text: &str, rect: UiRect, font_height: f32, font_weight: i32) -> Option<f32> {
    let request = TextLayoutRequest::single_line(text, rect, font_height, font_weight);
    layout(&request).map(|layout| layout.width).or_else(|| {
        measure(&TextMeasureRequest {
            text,
            bounds: rect,
            font_height,
            font_weight,
        })
        .map(|metrics| metrics.width)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedTextSystem;

    impl TextSystem for FixedTextSystem {
        fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
            Some(TextMetrics {
                width: request.text.len() as f32 * 3.0,
            })
        }

        fn layout(&self, request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
            let width = request.text.len() as f32 * 3.0;
            Some(TextLayout::new(
                width,
                10.0,
                false,
                vec![TextLineMetrics {
                    range: 0..request.text.chars().count(),
                    bounds: UiRect::new(0.0, 0.0, width, 10.0),
                    baseline: 8.0,
                    hard_break: false,
                }],
                Vec::new(),
                Vec::new(),
            ))
        }
    }

    #[test]
    fn text_system_capability_is_scoped_and_restored() {
        let bounds = UiRect::new(0.0, 0.0, 100.0, 20.0);
        assert_eq!(measure_width("abc", bounds, -14.0, 400), None);
        {
            let _guard = install_text_system(TextSystemHandle::new(FixedTextSystem));
            assert_eq!(measure_width("abc", bounds, -14.0, 400), Some(9.0));
        }
        assert_eq!(measure_width("abc", bounds, -14.0, 400), None);
    }

    #[test]
    fn selection_merges_adjacent_clusters_but_preserves_visual_runs() {
        let layout = TextLayout::new(
            30.0,
            10.0,
            false,
            vec![TextLineMetrics {
                range: 0..3,
                bounds: UiRect::new(0.0, 0.0, 30.0, 10.0),
                baseline: 8.0,
                hard_break: false,
            }],
            vec![
                TextCluster {
                    range: 0..1,
                    bounds: UiRect::new(0.0, 0.0, 10.0, 10.0),
                    direction: TextDirection::LeftToRight,
                },
                TextCluster {
                    range: 1..2,
                    bounds: UiRect::new(10.0, 0.0, 20.0, 10.0),
                    direction: TextDirection::LeftToRight,
                },
                TextCluster {
                    range: 2..3,
                    bounds: UiRect::new(40.0, 0.0, 50.0, 10.0),
                    direction: TextDirection::RightToLeft,
                },
            ],
            Vec::new(),
        );
        assert_eq!(
            layout.selection_rects(0..3),
            vec![
                UiRect::new(0.0, 0.0, 20.0, 10.0),
                UiRect::new(40.0, 0.0, 50.0, 10.0)
            ]
        );
    }
}
