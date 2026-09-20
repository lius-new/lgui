#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Self = Self(0x000000);
    pub const WHITE: Self = Self(0xFFFFFF);
}

use std::hash::{Hash, Hasher};

use super::geometry::normalized_f32_bits;
use crate::text::TextVerticalAlign;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub alpha: u8,
}

impl Stroke {
    pub const fn new(color: Color, width: f32, alpha: u8) -> Self {
        Self {
            color,
            width,
            alpha,
        }
    }
}

impl Eq for Stroke {}

impl Hash for Stroke {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        normalized_f32_bits(self.width).hash(state);
        self.alpha.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub height: f32,
    pub weight: i32,
    pub tracking: f32,
    pub align: TextAlign,
    pub vertical_align: TextVerticalAlign,
    pub alpha: u8,
}

impl TextStyle {
    pub const fn new(color: Color, height: f32, weight: i32) -> Self {
        Self {
            color,
            height,
            weight,
            tracking: 0.0,
            align: TextAlign::Left,
            vertical_align: TextVerticalAlign::CapCenter,
            alpha: 0xFF,
        }
    }

    pub const fn centered(mut self) -> Self {
        self.align = TextAlign::Center;
        self
    }

    pub const fn vertical_align(mut self, vertical_align: TextVerticalAlign) -> Self {
        self.vertical_align = vertical_align;
        self
    }

    pub const fn tracking(mut self, tracking: f32) -> Self {
        self.tracking = tracking;
        self
    }

    pub const fn alpha(mut self, alpha: u8) -> Self {
        self.alpha = alpha;
        self
    }
}

impl Eq for TextStyle {}

impl Hash for TextStyle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        normalized_f32_bits(self.height).hash(state);
        self.weight.hash(state);
        normalized_f32_bits(self.tracking).hash(state);
        self.align.hash(state);
        self.vertical_align.hash(state);
        self.alpha.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualStyle {
    pub fill: Option<Color>,
    pub fill_alpha: u8,
    pub stroke: Option<Stroke>,
    pub radius: f32,
}

impl Eq for VisualStyle {}

impl Hash for VisualStyle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fill.hash(state);
        self.fill_alpha.hash(state);
        self.stroke.hash(state);
        normalized_f32_bits(self.radius).hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PathStyle {
    pub fill: Option<Color>,
    pub fill_alpha: u8,
    pub stroke: Option<Stroke>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPathCommand {
    MoveTo(super::Point),
    LineTo(super::Point),
    QuadraticTo {
        control: super::Point,
        to: super::Point,
    },
    CubicTo {
        control1: super::Point,
        control2: super::Point,
        to: super::Point,
    },
    Close,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPath {
    pub commands: Vec<UiPathCommand>,
}

impl UiPath {
    pub fn new(commands: impl IntoIterator<Item = UiPathCommand>) -> Self {
        Self {
            commands: commands.into_iter().collect(),
        }
    }

    pub fn commands(&self) -> &[UiPathCommand] {
        &self.commands
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalGradientLayer {
    pub color: Color,
    pub alpha_top: f32,
    pub alpha_bottom: f32,
}

impl VerticalGradientLayer {
    pub const fn new(color: Color, alpha_top: f32, alpha_bottom: f32) -> Self {
        Self {
            color,
            alpha_top,
            alpha_bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialGradientLayer {
    pub color: Color,
    pub alpha: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

impl RadialGradientLayer {
    pub const fn new(color: Color, alpha: f32, center_x: f32, center_y: f32, radius: f32) -> Self {
        Self {
            color,
            alpha,
            center_x,
            center_y,
            radius,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverlayStyle {
    pub vertical_layers: Vec<VerticalGradientLayer>,
    pub radial_layers: Vec<RadialGradientLayer>,
}

impl OverlayStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertical(mut self, layer: VerticalGradientLayer) -> Self {
        self.vertical_layers.push(layer);
        self
    }

    pub fn radial(mut self, layer: RadialGradientLayer) -> Self {
        self.radial_layers.push(layer);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlurEdgeMode {
    /// Pixels sampled outside the affected region are treated as transparent.
    Transparent,
    /// Pixels sampled outside the affected region clamp to the nearest edge pixel.
    Clamp,
}

/// A portable Gaussian blur effect.
///
/// `sigma_x` and `sigma_y` are Gaussian standard deviations in logical UI units.
/// They are projected to the target coordinate space by the renderer, so an
/// effect stays visually consistent across scale factors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlurStyle {
    pub sigma_x: f32,
    pub sigma_y: f32,
    pub edge_mode: BlurEdgeMode,
    pub opacity: f32,
    pub tint: Color,
    pub tint_alpha: f32,
}

impl BlurStyle {
    pub const fn new(sigma: f32) -> Self {
        Self {
            sigma_x: sigma,
            sigma_y: sigma,
            edge_mode: BlurEdgeMode::Clamp,
            opacity: 1.0,
            tint: Color::BLACK,
            tint_alpha: 0.0,
        }
    }

    pub const fn sigma_x(mut self, sigma: f32) -> Self {
        self.sigma_x = sigma;
        self
    }

    pub const fn sigma_y(mut self, sigma: f32) -> Self {
        self.sigma_y = sigma;
        self
    }

    pub const fn edge_mode(mut self, mode: BlurEdgeMode) -> Self {
        self.edge_mode = mode;
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub const fn tint(mut self, tint: Color, alpha: f32) -> Self {
        self.tint = tint;
        self.tint_alpha = alpha;
        self
    }

    /// Largest Gaussian sigma across both axes, used to size blur outsets.
    pub fn sigma(self) -> f32 {
        self.sigma_x.max(self.sigma_y).max(0.0)
    }

    /// Amount by which the affected region must be expanded so blurred pixels
    /// are not clipped (approximately `3 * sigma` on each side).
    pub fn outset(self) -> f32 {
        3.0 * self.sigma()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomPaintStyle {
    pub color: Color,
    pub intensity: f32,
}

impl CustomPaintStyle {
    pub const fn new(color: Color, intensity: f32) -> Self {
        Self { color, intensity }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconStyle {
    pub color: Color,
    pub alpha: u8,
}

impl IconStyle {
    pub const fn new(color: Color) -> Self {
        Self { color, alpha: 0xFF }
    }

    pub const fn alpha(mut self, alpha: u8) -> Self {
        self.alpha = alpha;
        self
    }
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            fill: None,
            fill_alpha: 0xFF,
            stroke: None,
            radius: 0.0,
        }
    }
}

impl Default for PathStyle {
    fn default() -> Self {
        Self {
            fill: None,
            fill_alpha: 0xFF,
            stroke: None,
        }
    }
}

impl VisualStyle {
    pub const fn filled(fill: Color) -> Self {
        Self {
            fill: Some(fill),
            fill_alpha: 0xFF,
            stroke: None,
            radius: 0.0,
        }
    }

    pub const fn stroked(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub const fn alpha(mut self, alpha: u8) -> Self {
        self.fill_alpha = alpha;
        self
    }
}

impl PathStyle {
    pub const fn filled(fill: Color) -> Self {
        Self {
            fill: Some(fill),
            fill_alpha: 0xFF,
            stroke: None,
        }
    }

    pub const fn stroked(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub const fn alpha(mut self, alpha: u8) -> Self {
        self.fill_alpha = alpha;
        self
    }
}
