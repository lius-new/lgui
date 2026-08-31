use std::hash::{Hash, Hasher};

fn normalize(value: f32) -> f32 {
    if value.is_finite() {
        if value == 0.0 {
            0.0
        } else {
            value
        }
    } else {
        0.0
    }
}

fn hash_f32(value: f32, state: &mut impl Hasher) {
    normalize(value).to_bits().hash(state);
}

pub(crate) fn normalized_f32_bits(value: f32) -> u32 {
    normalize(value).to_bits()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn normalized(self) -> Self {
        Self::new(normalize(self.x), normalize(self.y))
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        self.normalized().x == other.normalized().x && self.normalized().y == other.normalized().y
    }
}

impl Eq for Point {}

impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_f32(self.x, state);
        hash_f32(self.y, state);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn normalized(self) -> Self {
        Self::new(
            normalize(self.width).max(0.0),
            normalize(self.height).max(0.0),
        )
    }
}

impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        self.normalized().width == other.normalized().width
            && self.normalized().height == other.normalized().height
    }
}

impl Eq for Size {}

impl Hash for Size {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_f32(self.normalized().width, state);
        hash_f32(self.normalized().height, state);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

impl PhysicalPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalSize {
    pub width: i32,
    pub height: i32,
}

impl PhysicalSize {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysicalRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn union(self, other: Self) -> Self {
        Self::new(
            self.left.min(other.left),
            self.top.min(other.top),
            self.right.max(other.right),
            self.bottom.max(other.bottom),
        )
    }

    pub fn intersect(self, other: Self) -> Option<Self> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right.min(other.right);
        let bottom = self.bottom.min(other.bottom);
        (left < right && top < bottom).then(|| Self::new(left, top, right, bottom))
    }

    pub fn as_ui_rect(self) -> UiRect {
        UiRect::new(
            self.left as f32,
            self.top as f32,
            self.right as f32,
            self.bottom as f32,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScale(f32);

impl UiScale {
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f32) -> Self {
        Self(if value.is_finite() && value > 0.0 {
            value
        } else {
            1.0
        })
    }

    pub fn factor(self) -> f32 {
        self.0
    }

    pub fn is_identity(self) -> bool {
        (self.0 - 1.0).abs() < f32::EPSILON
    }

    pub fn physical_value(self, logical: f32) -> i32 {
        (normalize(logical) * self.0).round() as i32
    }

    pub fn physical_ui_value(self, logical: f32) -> f32 {
        normalize(logical) * self.0
    }

    pub fn physical_ui_length(self, logical: f32) -> f32 {
        self.physical_ui_value(logical).max(0.0)
    }

    pub fn physical_ui_signed_length(self, logical: f32) -> f32 {
        self.physical_ui_value(logical)
    }

    pub fn physical_length(self, logical: f32) -> i32 {
        if logical <= 0.0 {
            return 0;
        }
        self.physical_value(logical).max(1)
    }

    pub fn physical_signed_length(self, logical: f32) -> i32 {
        if logical < 0.0 {
            -self.physical_length(logical.abs())
        } else if logical > 0.0 {
            self.physical_length(logical)
        } else {
            0
        }
    }

    pub fn logical_value(self, physical: i32) -> f32 {
        physical as f32 / self.0
    }

    pub fn physical_point(self, logical: Point) -> PhysicalPoint {
        PhysicalPoint::new(
            self.physical_value(logical.x),
            self.physical_value(logical.y),
        )
    }

    pub fn physical_ui_point(self, logical: Point) -> Point {
        Point::new(logical.x * self.0, logical.y * self.0)
    }

    pub fn logical_point(self, physical: PhysicalPoint) -> Point {
        Point::new(
            self.logical_value(physical.x),
            self.logical_value(physical.y),
        )
    }

    pub fn physical_rect(self, logical: UiRect) -> PhysicalRect {
        let mut left = self.physical_value(logical.left);
        let mut top = self.physical_value(logical.top);
        let mut right = self.physical_value(logical.right);
        let mut bottom = self.physical_value(logical.bottom);

        if logical.right > logical.left && right == left {
            left = (logical.left * self.0).floor() as i32;
            right = (logical.right * self.0).ceil() as i32;
        }
        if logical.bottom > logical.top && bottom == top {
            top = (logical.top * self.0).floor() as i32;
            bottom = (logical.bottom * self.0).ceil() as i32;
        }

        PhysicalRect::new(left, top, right, bottom)
    }

    pub fn physical_rect_outward(self, logical: UiRect) -> PhysicalRect {
        PhysicalRect::new(
            (logical.left * self.0).floor() as i32,
            (logical.top * self.0).floor() as i32,
            (logical.right * self.0).ceil() as i32,
            (logical.bottom * self.0).ceil() as i32,
        )
    }

    pub fn physical_ui_rect(self, logical: UiRect) -> UiRect {
        UiRect::new(
            logical.left * self.0,
            logical.top * self.0,
            logical.right * self.0,
            logical.bottom * self.0,
        )
    }

    pub fn logical_rect(self, physical: PhysicalRect) -> UiRect {
        UiRect::new(
            self.logical_value(physical.left),
            self.logical_value(physical.top),
            self.logical_value(physical.right),
            self.logical_value(physical.bottom),
        )
    }

    pub fn logical_size(self, physical: PhysicalSize) -> Size {
        Size::new(
            self.logical_value(physical.width).max(1.0),
            self.logical_value(physical.height).max(1.0),
        )
    }

    pub fn physical_size(self, logical: Size) -> PhysicalSize {
        PhysicalSize::new(
            self.physical_length(logical.width),
            self.physical_length(logical.height),
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub const ZERO: Self = Self::all(0.0);

    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }
}

impl PartialEq for EdgeInsets {
    fn eq(&self, other: &Self) -> bool {
        normalize(self.left) == normalize(other.left)
            && normalize(self.top) == normalize(other.top)
            && normalize(self.right) == normalize(other.right)
            && normalize(self.bottom) == normalize(other.bottom)
    }
}

impl Eq for EdgeInsets {}

impl Hash for EdgeInsets {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_f32(self.left, state);
        hash_f32(self.top, state);
        hash_f32(self.right, state);
        hash_f32(self.bottom, state);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UiRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl UiRect {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn normalized(self) -> Self {
        let left = normalize(self.left);
        let top = normalize(self.top);
        let right = normalize(self.right);
        let bottom = normalize(self.bottom);
        Self::new(
            left.min(right),
            top.min(bottom),
            left.max(right),
            top.max(bottom),
        )
    }

    pub fn width(self) -> f32 {
        self.right - self.left
    }

    pub fn height(self) -> f32 {
        self.bottom - self.top
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    pub fn inflate(self, x: f32, y: f32) -> Self {
        Self::new(self.left - x, self.top - y, self.right + x, self.bottom + y)
    }

    pub fn translate(self, x: f32, y: f32) -> Self {
        Self::new(self.left + x, self.top + y, self.right + x, self.bottom + y)
    }

    pub fn inset(self, insets: EdgeInsets) -> Self {
        Self::new(
            self.left + insets.left,
            self.top + insets.top,
            self.right - insets.right,
            self.bottom - insets.bottom,
        )
    }

    pub fn union(self, other: Self) -> Self {
        Self::new(
            self.left.min(other.left),
            self.top.min(other.top),
            self.right.max(other.right),
            self.bottom.max(other.bottom),
        )
    }

    pub fn intersect(self, other: Self) -> Option<Self> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right.min(other.right);
        let bottom = self.bottom.min(other.bottom);
        (left < right && top < bottom).then(|| Self::new(left, top, right, bottom))
    }
}

impl PartialEq for UiRect {
    fn eq(&self, other: &Self) -> bool {
        self.normalized().left == other.normalized().left
            && self.normalized().top == other.normalized().top
            && self.normalized().right == other.normalized().right
            && self.normalized().bottom == other.normalized().bottom
    }
}

impl Eq for UiRect {}

impl Hash for UiRect {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let rect = self.normalized();
        hash_f32(rect.left, state);
        hash_f32(rect.top, state);
        hash_f32(rect.right, state);
        hash_f32(rect.bottom, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_round_trip_preserves_fractional_logical_points() {
        for factor in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let scale = UiScale::new(factor);
            let logical = Point::new(317.25, 241.5);
            let round_trip = scale.logical_point(scale.physical_point(logical));
            assert!((round_trip.x - logical.x).abs() <= 1.0 / factor);
            assert!((round_trip.y - logical.y).abs() <= 1.0 / factor);
        }
    }

    #[test]
    fn dirty_rect_projection_never_shrinks_the_covered_area() {
        let scale = UiScale::new(1.25);
        assert_eq!(
            scale.physical_rect_outward(UiRect::new(1.0, 1.0, 3.0, 3.0)),
            PhysicalRect::new(1, 1, 4, 4)
        );
    }

    #[test]
    fn physical_rect_keeps_downscaled_hairlines_visible() {
        let scale = UiScale::new(0.5);
        assert_eq!(
            scale.physical_rect(UiRect::new(0.0, 75.0, 100.0, 76.0)),
            PhysicalRect::new(0, 37, 50, 38)
        );
    }

    #[test]
    fn invalid_logical_geometry_normalizes_before_use() {
        assert_eq!(
            UiRect::new(f32::NAN, 20.0, f32::INFINITY, 10.0).normalized(),
            UiRect::new(0.0, 10.0, 0.0, 20.0)
        );
    }

    #[test]
    fn signed_lengths_preserve_win32_font_height_semantics() {
        let scale = UiScale::new(1.5);
        assert_eq!(scale.physical_signed_length(-11.0), -17);
        assert_eq!(scale.physical_signed_length(11.0), 17);
        assert_eq!(scale.physical_signed_length(0.0), 0);
    }
}
