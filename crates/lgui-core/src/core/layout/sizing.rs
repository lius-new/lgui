use std::hash::{Hash, Hasher};

use super::Size;

/// A logical dimension. Percentages are fractions: `Percent(0.5)` is 50%.
/// `Auto` preserves the declared size, while `Content` asks the layout runtime
/// to measure the node's content using the installed text system and its children.
#[derive(Clone, Copy, Debug, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Points(f32),
    Percent(f32),
    Content,
}

impl Dimension {
    fn key(self) -> (u8, u32) {
        match self {
            Self::Auto => (0, 0),
            Self::Points(value) => (1, finite_length(value).to_bits()),
            Self::Percent(value) => (2, finite_length(value).to_bits()),
            Self::Content => (3, 0),
        }
    }

    fn resolve(self, available: f32, preferred: f32, intrinsic: f32) -> f32 {
        match self {
            Self::Auto => preferred,
            Self::Points(value) => finite_length(value),
            Self::Percent(value) => saturated_product(available, finite_length(value)),
            Self::Content => intrinsic,
        }
    }

    fn definite(self) -> bool {
        matches!(self, Self::Points(_) | Self::Percent(_))
    }
}

impl PartialEq for Dimension {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Dimension {}

impl Hash for Dimension {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// Child-owned sizing, independent of how the parent arranges its children.
/// A missing maximum is unbounded. Minimums win over contradictory maximums;
/// explicit dimensions and min/max bounds take precedence over an aspect ratio.
#[derive(Clone, Copy, Debug, Default)]
pub struct SizeConstraints {
    pub width: Dimension,
    pub height: Dimension,
    pub min: Size,
    pub max: Option<Size>,
    /// Preferred width divided by height. Non-positive or non-finite ratios are ignored.
    pub aspect_ratio: Option<f32>,
}

impl SizeConstraints {
    pub const fn new(width: Dimension, height: Dimension) -> Self {
        Self {
            width,
            height,
            min: Size::new(0.0, 0.0),
            max: None,
            aspect_ratio: None,
        }
    }

    pub const fn min_size(mut self, size: Size) -> Self {
        self.min = size;
        self
    }

    pub const fn max_size(mut self, size: Size) -> Self {
        self.max = Some(size);
        self
    }

    pub const fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }

    pub(crate) fn measures_content(self) -> bool {
        matches!(self.width, Dimension::Content) || matches!(self.height, Dimension::Content)
    }

    /// Resolves against the parent's content size, the element's declared size,
    /// and its measured content. Invalid lengths become zero; arithmetic saturates
    /// rather than introducing infinite geometry into hit testing or painting.
    pub fn resolve(self, available: Size, preferred: Size, intrinsic: Size) -> Size {
        let available = available.normalized();
        let preferred = preferred.normalized();
        let intrinsic = intrinsic.normalized();
        let min = self.min.normalized();
        let max = self
            .max
            .map(Size::normalized)
            .unwrap_or(Size::new(f32::MAX, f32::MAX));
        let max = Size::new(max.width.max(min.width), max.height.max(min.height));
        let mut width = self
            .width
            .resolve(available.width, preferred.width, intrinsic.width);
        let mut height = self
            .height
            .resolve(available.height, preferred.height, intrinsic.height);
        width = width.clamp(min.width, max.width);
        height = height.clamp(min.height, max.height);

        if let Some(ratio) = valid_ratio(self.aspect_ratio) {
            match (self.width.definite(), self.height.definite()) {
                (true, true) => {}
                (true, false) => {
                    height = (width / ratio).min(f32::MAX).clamp(min.height, max.height)
                }
                (false, true) => {
                    width = saturated_product(height, ratio).clamp(min.width, max.width)
                }
                (false, false) => {
                    let lower = min.width.max(saturated_product(min.height, ratio));
                    let upper = max.width.min(saturated_product(max.height, ratio));
                    let preferred = if width > 0.0 {
                        width
                    } else {
                        saturated_product(height, ratio)
                    };
                    if lower <= upper {
                        width = preferred.clamp(lower, upper);
                        height = (width / ratio).min(f32::MAX).clamp(min.height, max.height);
                    }
                }
            }
        }
        Size::new(width, height)
    }
}

impl PartialEq for SizeConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.min == other.min
            && self.max == other.max
            && valid_ratio(self.aspect_ratio).map(f32::to_bits)
                == valid_ratio(other.aspect_ratio).map(f32::to_bits)
    }
}

impl Eq for SizeConstraints {}

impl Hash for SizeConstraints {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.hash(state);
        self.height.hash(state);
        self.min.hash(state);
        self.max.hash(state);
        valid_ratio(self.aspect_ratio).map(f32::to_bits).hash(state);
    }
}

fn finite_length(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0) + 0.0
    } else {
        0.0
    }
}

fn saturated_product(left: f32, right: f32) -> f32 {
    (left * right).min(f32::MAX)
}

fn valid_ratio(ratio: Option<f32>) -> Option<f32> {
    ratio.filter(|value| value.is_finite() && *value > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_and_content_use_different_measurements() {
        let sizing = SizeConstraints::new(Dimension::Auto, Dimension::Content);
        assert_eq!(
            sizing.resolve(
                Size::new(300.0, 200.0),
                Size::new(40.0, 20.0),
                Size::new(80.0, 30.0)
            ),
            Size::new(40.0, 30.0)
        );
    }

    #[test]
    fn percentages_use_parent_content_and_respect_bounds() {
        let sizing = SizeConstraints::new(Dimension::Percent(0.5), Dimension::Points(30.0))
            .min_size(Size::new(80.0, 40.0))
            .max_size(Size::new(120.0, 60.0));
        assert_eq!(
            sizing.resolve(Size::new(400.0, 200.0), Size::default(), Size::default()),
            Size::new(120.0, 40.0)
        );
        assert_eq!(
            sizing.resolve(Size::new(100.0, 200.0), Size::default(), Size::default()),
            Size::new(80.0, 40.0)
        );
    }

    #[test]
    fn ratio_derives_the_automatic_axis_but_not_two_explicit_axes() {
        let sizing =
            SizeConstraints::new(Dimension::Points(120.0), Dimension::Auto).aspect_ratio(2.0);
        assert_eq!(
            sizing.resolve(Size::default(), Size::default(), Size::default()),
            Size::new(120.0, 60.0)
        );
        let sizing = SizeConstraints {
            height: Dimension::Points(40.0),
            ..sizing
        };
        assert_eq!(
            sizing.resolve(Size::default(), Size::default(), Size::default()),
            Size::new(120.0, 40.0)
        );
    }

    #[test]
    fn content_ratio_fits_both_axes_and_minimums_win_over_maximums() {
        let sizing = SizeConstraints::new(Dimension::Content, Dimension::Content)
            .aspect_ratio(2.0)
            .max_size(Size::new(100.0, 30.0));
        assert_eq!(
            sizing.resolve(Size::default(), Size::default(), Size::new(200.0, 100.0)),
            Size::new(60.0, 30.0)
        );
        let sizing = sizing.min_size(Size::new(120.0, 40.0));
        assert_eq!(
            sizing.resolve(Size::default(), Size::default(), Size::default()),
            Size::new(120.0, 40.0)
        );
    }

    #[test]
    fn invalid_numbers_have_finite_geometry_and_consistent_identity() {
        let sizing = SizeConstraints::new(Dimension::Points(f32::NAN), Dimension::Percent(-1.0))
            .aspect_ratio(f32::INFINITY);
        assert_eq!(sizing, sizing);
        assert_eq!(
            sizing.resolve(Size::new(200.0, 200.0), Size::default(), Size::default()),
            Size::default()
        );
        let sizing = SizeConstraints::new(Dimension::Percent(f32::MAX), Dimension::Auto);
        assert_eq!(
            sizing
                .resolve(Size::new(200.0, 200.0), Size::default(), Size::default())
                .width,
            f32::MAX
        );
        use std::collections::hash_map::DefaultHasher;
        let mut negative = DefaultHasher::new();
        let mut zero = DefaultHasher::new();
        Dimension::Points(-0.0).hash(&mut negative);
        Dimension::Points(0.0).hash(&mut zero);
        assert_eq!(negative.finish(), zero.finish());
    }
}
