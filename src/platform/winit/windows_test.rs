use super::*;

#[test]
fn corner_radius_maps_to_the_closest_dwm_preference() {
    assert_eq!(
        corner_preference(0, WindowMode::Windowed),
        CornerPreference::DoNotRound
    );
    assert_eq!(
        corner_preference(4, WindowMode::Windowed),
        CornerPreference::RoundSmall
    );
    assert_eq!(
        corner_preference(8, WindowMode::Windowed),
        CornerPreference::Round
    );
}

#[test]
fn fullscreen_windows_disable_dwm_rounding() {
    assert_eq!(
        corner_preference(8, WindowMode::Fullscreen),
        CornerPreference::DoNotRound
    );
}
