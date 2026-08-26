#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: i32,
    pub height: i32,
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

    pub fn physical_value(self, logical: i32) -> i32 {
        (logical as f32 * self.0).round() as i32
    }

    pub fn physical_length(self, logical: i32) -> i32 {
        if logical <= 0 {
            return 0;
        }
        self.physical_value(logical).max(1)
    }

    pub fn physical_signed_length(self, logical: i32) -> i32 {
        match logical.cmp(&0) {
            std::cmp::Ordering::Less => -self.physical_length(logical.saturating_abs()),
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => self.physical_length(logical),
        }
    }

    pub fn logical_value(self, physical: i32) -> i32 {
        (physical as f32 / self.0).floor() as i32
    }

    pub fn physical_point(self, logical: Point) -> Point {
        Point::new(
            self.physical_value(logical.x),
            self.physical_value(logical.y),
        )
    }

    pub fn logical_point(self, physical: Point) -> Point {
        Point::new(
            self.logical_value(physical.x),
            self.logical_value(physical.y),
        )
    }

    pub fn physical_rect(self, logical: UiRect) -> UiRect {
        let mut left = self.physical_value(logical.left);
        let mut top = self.physical_value(logical.top);
        let mut right = self.physical_value(logical.right);
        let mut bottom = self.physical_value(logical.bottom);

        // Independent edge rounding can collapse a one-pixel hairline at fractional scales.
        // Only expand axes that actually collapsed so regular rectangles keep their alignment.
        if logical.right > logical.left && right == left {
            left = (logical.left as f32 * self.0).floor() as i32;
            right = (logical.right as f32 * self.0).ceil() as i32;
        }
        if logical.bottom > logical.top && bottom == top {
            top = (logical.top as f32 * self.0).floor() as i32;
            bottom = (logical.bottom as f32 * self.0).ceil() as i32;
        }

        UiRect::new(left, top, right, bottom)
    }

    pub fn physical_rect_outward(self, logical: UiRect) -> UiRect {
        UiRect::new(
            (logical.left as f32 * self.0).floor() as i32,
            (logical.top as f32 * self.0).floor() as i32,
            (logical.right as f32 * self.0).ceil() as i32,
            (logical.bottom as f32 * self.0).ceil() as i32,
        )
    }

    pub fn logical_size(self, physical: Size) -> Size {
        Size::new(
            self.logical_value(physical.width).max(1),
            self.logical_value(physical.height).max(1),
        )
    }

    pub fn physical_size(self, logical: Size) -> Size {
        Size::new(
            self.physical_length(logical.width),
            self.physical_length(logical.height),
        )
    }
}

impl Size {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EdgeInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl EdgeInsets {
    pub const ZERO: Self = Self::all(0);

    pub const fn all(value: i32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub const fn symmetric(horizontal: i32, vertical: i32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UiRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl UiRect {
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

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    pub fn inflate(self, x: i32, y: i32) -> Self {
        Self::new(self.left - x, self.top - y, self.right + x, self.bottom + y)
    }

    pub fn translate(self, x: i32, y: i32) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_round_trip_keeps_points_within_one_logical_pixel() {
        for factor in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let scale = UiScale::new(factor);
            let logical = Point::new(317, 241);
            let round_trip = scale.logical_point(scale.physical_point(logical));
            assert!((round_trip.x - logical.x).abs() <= 1);
            assert!((round_trip.y - logical.y).abs() <= 1);
        }
    }

    #[test]
    fn dirty_rect_projection_never_shrinks_the_covered_area() {
        let scale = UiScale::new(1.25);
        assert_eq!(
            scale.physical_rect_outward(UiRect::new(1, 1, 3, 3)),
            UiRect::new(1, 1, 4, 4)
        );
    }

    #[test]
    fn physical_rect_keeps_downscaled_hairlines_visible() {
        let scale = UiScale::new(0.5);

        assert_eq!(
            scale.physical_rect(UiRect::new(0, 75, 100, 76)),
            UiRect::new(0, 37, 50, 38)
        );
        assert_eq!(
            scale.physical_rect(UiRect::new(75, 0, 76, 100)),
            UiRect::new(37, 0, 38, 50)
        );
    }

    #[test]
    fn signed_lengths_preserve_win32_font_height_semantics() {
        let scale = UiScale::new(1.5);
        assert_eq!(scale.physical_signed_length(-11), -17);
        assert_eq!(scale.physical_signed_length(11), 17);
        assert_eq!(scale.physical_signed_length(0), 0);
    }
}
