#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CompositingLayerBackground {
    Opaque,
    #[default]
    Transparent,
}

const ROTATION_UNITS_PER_DEGREE: f32 = 1_000.0;
const TRANSFORM_UNITS: f32 = 1_000_000.0;
const TRANSFORM_UNITS_I32: i32 = 1_000_000;
const TRANSLATION_UNITS: f32 = 1_000.0;
const MAX_SCALE: f32 = 64.0;
const MAX_TRANSLATION: f32 = 1_000_000.0;

/// A backend-neutral transform applied while a retained layer is composited.
///
/// Values are quantized internally so animated transforms remain safe to compare and hash.
/// The retained layer contents are not rerasterized when only this transform changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerTransform {
    rotation_millidegrees: i32,
    scale_x_micros: i32,
    scale_y_micros: i32,
    origin_x_micros: i32,
    origin_y_micros: i32,
    translation_x_millis: i32,
    translation_y_millis: i32,
}

impl LayerTransform {
    pub const fn identity() -> Self {
        Self {
            rotation_millidegrees: 0,
            scale_x_micros: TRANSFORM_UNITS_I32,
            scale_y_micros: TRANSFORM_UNITS_I32,
            origin_x_micros: TRANSFORM_UNITS_I32 / 2,
            origin_y_micros: TRANSFORM_UNITS_I32 / 2,
            translation_x_millis: 0,
            translation_y_millis: 0,
        }
    }

    pub fn rotation_degrees(mut self, degrees: f32) -> Self {
        self.rotation_millidegrees = quantize_rotation(degrees);
        self
    }

    pub fn rotation_radians(self, radians: f32) -> Self {
        self.rotation_degrees(radians.to_degrees())
    }

    pub fn scale(mut self, scale: f32) -> Self {
        let scale = quantize_scale(scale);
        self.scale_x_micros = scale;
        self.scale_y_micros = scale;
        self
    }

    pub fn scale_xy(mut self, scale_x: f32, scale_y: f32) -> Self {
        self.scale_x_micros = quantize_scale(scale_x);
        self.scale_y_micros = quantize_scale(scale_y);
        self
    }

    /// Moves the composited layer without changing its retained contents.
    pub fn translation(mut self, x: f32, y: f32) -> Self {
        self.translation_x_millis = quantize_translation(x);
        self.translation_y_millis = quantize_translation(y);
        self
    }

    /// Sets the transform origin in normalized layer coordinates.
    pub fn origin(mut self, x: f32, y: f32) -> Self {
        self.origin_x_micros = quantize_origin(x);
        self.origin_y_micros = quantize_origin(y);
        self
    }

    pub fn rotation_degrees_f32(self) -> f32 {
        self.rotation_millidegrees as f32 / ROTATION_UNITS_PER_DEGREE
    }

    pub fn rotation_radians_f32(self) -> f32 {
        self.rotation_degrees_f32().to_radians()
    }

    pub fn scale_x(self) -> f32 {
        self.scale_x_micros as f32 / TRANSFORM_UNITS
    }

    pub fn scale_y(self) -> f32 {
        self.scale_y_micros as f32 / TRANSFORM_UNITS
    }

    pub fn origin_x(self) -> f32 {
        self.origin_x_micros as f32 / TRANSFORM_UNITS
    }

    pub fn origin_y(self) -> f32 {
        self.origin_y_micros as f32 / TRANSFORM_UNITS
    }

    pub fn translation_x(self) -> f32 {
        self.translation_x_millis as f32 / TRANSLATION_UNITS
    }

    pub fn translation_y(self) -> f32 {
        self.translation_y_millis as f32 / TRANSLATION_UNITS
    }

    pub(crate) fn project_to_physical(self, scale: super::UiScale) -> Self {
        self.translation(
            self.translation_x() * scale.factor(),
            self.translation_y() * scale.factor(),
        )
    }

    pub const fn is_identity(self) -> bool {
        self.rotation_millidegrees == 0
            && self.scale_x_micros == TRANSFORM_UNITS_I32
            && self.scale_y_micros == TRANSFORM_UNITS_I32
            && self.translation_x_millis == 0
            && self.translation_y_millis == 0
    }

    pub fn transformed_bounds(self, rect: super::UiRect) -> super::UiRect {
        if self.is_identity() {
            return rect;
        }
        let points = [
            self.transform_point(rect, rect.left as f32, rect.top as f32),
            self.transform_point(rect, rect.right as f32, rect.top as f32),
            self.transform_point(rect, rect.left as f32, rect.bottom as f32),
            self.transform_point(rect, rect.right as f32, rect.bottom as f32),
        ];
        let min_x = points
            .iter()
            .map(|point| point.0)
            .fold(f32::INFINITY, f32::min);
        let min_y = points
            .iter()
            .map(|point| point.1)
            .fold(f32::INFINITY, f32::min);
        let max_x = points
            .iter()
            .map(|point| point.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f32::NEG_INFINITY, f32::max);
        super::UiRect::new(min_x, min_y, max_x, max_y)
    }

    pub(crate) fn transform_point(self, rect: super::UiRect, x: f32, y: f32) -> (f32, f32) {
        let origin_x = rect.left as f32 + rect.width() as f32 * self.origin_x();
        let origin_y = rect.top as f32 + rect.height() as f32 * self.origin_y();
        let dx = (x - origin_x) * self.scale_x();
        let dy = (y - origin_y) * self.scale_y();
        let angle = self.rotation_radians_f32();
        let (sin, cos) = angle.sin_cos();
        (
            origin_x + dx * cos - dy * sin + self.translation_x(),
            origin_y + dx * sin + dy * cos + self.translation_y(),
        )
    }
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self::identity()
    }
}

fn quantize_rotation(degrees: f32) -> i32 {
    if !degrees.is_finite() {
        return 0;
    }
    let units_per_turn = (360.0 * ROTATION_UNITS_PER_DEGREE) as i32;
    ((degrees.rem_euclid(360.0) * ROTATION_UNITS_PER_DEGREE).round() as i32)
        .rem_euclid(units_per_turn)
}

fn quantize_scale(scale: f32) -> i32 {
    if !scale.is_finite() {
        return TRANSFORM_UNITS_I32;
    }
    (scale.clamp(-MAX_SCALE, MAX_SCALE) * TRANSFORM_UNITS).round() as i32
}

fn quantize_origin(origin: f32) -> i32 {
    if !origin.is_finite() {
        return TRANSFORM_UNITS_I32 / 2;
    }
    (origin.clamp(0.0, 1.0) * TRANSFORM_UNITS).round() as i32
}

fn quantize_translation(translation: f32) -> i32 {
    if !translation.is_finite() {
        return 0;
    }
    (translation.clamp(-MAX_TRANSLATION, MAX_TRANSLATION) * TRANSLATION_UNITS).round() as i32
}

/// An independently retained paint surface. Animation is only one possible producer of changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompositingLayerSpec {
    pub opacity: u8,
    pub background: CompositingLayerBackground,
    pub transform: LayerTransform,
    // The scene compiler reserves transparent padding before assigning this effect.
    pub(crate) shadow: Option<super::ShadowStyle>,
}

impl CompositingLayerSpec {
    pub const fn new() -> Self {
        Self {
            opacity: 255,
            background: CompositingLayerBackground::Transparent,
            transform: LayerTransform::identity(),
            shadow: None,
        }
    }

    pub const fn opaque(mut self) -> Self {
        self.background = CompositingLayerBackground::Opaque;
        self
    }

    pub const fn transparent(mut self) -> Self {
        self.background = CompositingLayerBackground::Transparent;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        self
    }

    pub fn opacity_f32(self) -> f32 {
        self.opacity as f32 / 255.0
    }

    pub fn transform(mut self, transform: LayerTransform) -> Self {
        self.transform = transform;
        self
    }

    pub fn rotation_degrees(mut self, degrees: f32) -> Self {
        self.transform = self.transform.rotation_degrees(degrees);
        self
    }

    pub fn rotation_radians(mut self, radians: f32) -> Self {
        self.transform = self.transform.rotation_radians(radians);
        self
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.transform = self.transform.scale(scale);
        self
    }

    pub fn scale_xy(mut self, scale_x: f32, scale_y: f32) -> Self {
        self.transform = self.transform.scale_xy(scale_x, scale_y);
        self
    }

    pub fn transform_origin(mut self, x: f32, y: f32) -> Self {
        self.transform = self.transform.origin(x, y);
        self
    }

    pub fn translation(mut self, x: f32, y: f32) -> Self {
        self.transform = self.transform.translation(x, y);
        self
    }
}

impl Default for CompositingLayerSpec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "compositing_layer_test.rs"]
mod tests;
