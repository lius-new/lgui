use super::*;

#[test]
fn wheel_delta_preserves_fractional_line_and_pixel_input() {
    let lines = WheelDelta::lines(0.25, -1.5);
    assert_eq!(
        (lines.x, lines.y, lines.unit),
        (0.25, -1.5, WheelUnit::Lines)
    );

    let pixels = WheelDelta::pixels(1.75, -3.25);
    assert_eq!(
        (pixels.x, pixels.y, pixels.unit),
        (1.75, -3.25, WheelUnit::Pixels)
    );
}
