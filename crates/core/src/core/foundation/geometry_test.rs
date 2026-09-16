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
