use super::{Color, UiRect, UiScale};

/// A drop shadow of an element's composited alpha, including its descendants.
/// Distances are logical pixels; blur is the Gaussian standard deviation.
/// Positive spread dilates the alpha mask; negative spread erodes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowStyle {
    pub color: Color,
    pub alpha: u8,
    offset_x_millis: i32,
    offset_y_millis: i32,
    blur_millis: i32,
    spread_millis: i32,
}

impl ShadowStyle {
    pub const fn new(color: Color) -> Self {
        Self {
            color,
            alpha: 64,
            offset_x_millis: 0,
            offset_y_millis: 4000,
            blur_millis: 6000,
            spread_millis: 0,
        }
    }

    pub const fn alpha(mut self, alpha: u8) -> Self {
        self.alpha = alpha;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x_millis = distance(x);
        self.offset_y_millis = distance(y);
        self
    }

    pub fn blur(mut self, sigma: f32) -> Self {
        self.blur_millis = distance(sigma).max(0);
        self
    }

    pub fn spread(mut self, spread: f32) -> Self {
        self.spread_millis = distance(spread);
        self
    }

    pub fn offset_x(self) -> f32 {
        self.offset_x_millis as f32 / 1000.0
    }
    pub fn offset_y(self) -> f32 {
        self.offset_y_millis as f32 / 1000.0
    }
    pub fn blur_sigma(self) -> f32 {
        self.blur_millis as f32 / 1000.0
    }
    pub fn spread_radius(self) -> f32 {
        self.spread_millis as f32 / 1000.0
    }

    pub fn paint_bounds(self, content: UiRect) -> UiRect {
        if self.alpha == 0 {
            return content;
        }
        // Four sigma covers the Gaussian kernel support; reserve another sampling pixel.
        let outset = (self.blur_sigma() * 4.0).ceil() + self.spread_radius().max(0.0).ceil() + 1.0;
        let padded = content.inflate(outset, outset);
        padded.union(padded.translate(self.offset_x(), self.offset_y()))
    }

    pub(crate) fn project_to_physical(self, scale: UiScale) -> Self {
        self.offset(
            scale.physical_ui_value(self.offset_x()),
            scale.physical_ui_value(self.offset_y()),
        )
        .blur(scale.physical_ui_value(self.blur_sigma()))
        .spread(scale.physical_ui_value(self.spread_radius()))
    }
}

impl Default for ShadowStyle {
    fn default() -> Self {
        Self::new(Color::BLACK)
    }
}

fn distance(value: f32) -> i32 {
    if value.is_finite() {
        (value.clamp(-4096.0, 4096.0) * 1000.0).round() as i32
    } else {
        0
    }
}
