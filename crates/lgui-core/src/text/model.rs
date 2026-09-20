use std::ops::Range;

use crate::core::{TextAlign, UiRect};

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
    /// Centers the cap-height (uppercase letter / digit) visual box in the bounds.
    CapCenter,
    /// Centers the x-height (lowercase letter) visual box in the bounds.
    XCenter,
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
