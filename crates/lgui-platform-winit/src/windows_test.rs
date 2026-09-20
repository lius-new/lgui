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

#[test]
fn resize_directions_map_to_the_matching_hit_test_codes() {
    assert_eq!(hit_test_for_resize(ResizeDirection::North), HTTOP);
    assert_eq!(hit_test_for_resize(ResizeDirection::South), HTBOTTOM);
    assert_eq!(hit_test_for_resize(ResizeDirection::East), HTRIGHT);
    assert_eq!(hit_test_for_resize(ResizeDirection::West), HTLEFT);
    assert_eq!(hit_test_for_resize(ResizeDirection::NorthEast), HTTOPRIGHT);
    assert_eq!(hit_test_for_resize(ResizeDirection::NorthWest), HTTOPLEFT);
    assert_eq!(hit_test_for_resize(ResizeDirection::SouthEast), HTBOTTOMRIGHT);
    assert_eq!(hit_test_for_resize(ResizeDirection::SouthWest), HTBOTTOMLEFT);
}
