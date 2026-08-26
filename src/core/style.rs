#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Self = Self(0x000000);
    pub const WHITE: Self = Self(0xFFFFFF);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub color: Color,
    pub width: i32,
    pub alpha: u8,
}

impl Stroke {
    pub const fn new(color: Color, width: i32, alpha: u8) -> Self {
        Self {
            color,
            width,
            alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub color: Color,
    pub height: i32,
    pub weight: i32,
    pub tracking: i32,
    pub align: TextAlign,
    pub alpha: u8,
}

impl TextStyle {
    pub const fn new(color: Color, height: i32, weight: i32) -> Self {
        Self {
            color,
            height,
            weight,
            tracking: 0,
            align: TextAlign::Left,
            alpha: 0xFF,
        }
    }

    pub const fn centered(mut self) -> Self {
        self.align = TextAlign::Center;
        self
    }

    pub const fn tracking(mut self, tracking: i32) -> Self {
        self.tracking = tracking;
        self
    }

    pub const fn alpha(mut self, alpha: u8) -> Self {
        self.alpha = alpha;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualStyle {
    pub fill: Option<Color>,
    pub fill_alpha: u8,
    pub stroke: Option<Stroke>,
    pub radius: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropBlurStyle {
    pub source: &'static str,
    pub fit: super::ImageFit,
    pub source_rect: super::UiRect,
    pub radius: usize,
    pub opacity: f32,
    pub tint: Color,
    pub tint_alpha: f32,
}

impl BackdropBlurStyle {
    pub const fn new(
        source: &'static str,
        fit: super::ImageFit,
        source_rect: super::UiRect,
    ) -> Self {
        Self {
            source,
            fit,
            source_rect,
            radius: 24,
            opacity: 1.0,
            tint: Color::BLACK,
            tint_alpha: 0.0,
        }
    }

    pub const fn radius(mut self, radius: usize) -> Self {
        self.radius = radius;
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
            radius: 0,
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
            radius: 0,
        }
    }

    pub const fn stroked(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub const fn radius(mut self, radius: i32) -> Self {
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
