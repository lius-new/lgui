use super::{
    math::*,
    model::{CHANGE_EVENT, COMMIT_EVENT},
    state::SliderState,
};
use crate::core::{
    ComponentState, UiAction, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION, POINTER_UP_ACTION,
};

#[test]
fn pointer_position_maps_to_continuous_values_and_clamps() {
    assert!(same_value(
        value_from_pointer(50.0, 0.0, 200.0, -1.0, 1.0, None),
        -0.5
    ));
    assert!(same_value(
        value_from_pointer(-20.0, 0.0, 200.0, -1.0, 1.0, None),
        -1.0
    ));
    assert!(same_value(
        value_from_pointer(240.0, 0.0, 200.0, -1.0, 1.0, None),
        1.0
    ));
}

#[test]
fn optional_step_quantizes_relative_to_the_minimum() {
    assert!(same_value(
        value_from_pointer(44.0, 0.0, 100.0, 10.0, 20.0, Some(0.5)),
        14.5
    ));
    assert!(same_value(
        quantize_value(19.9, 10.0, 20.0, Some(3.0)),
        19.0
    ));
}

#[test]
fn slider_state_emits_dragged_values_and_commits_the_release_value() {
    let mut state = SliderState::default();
    state.configure(20.0, 0.0, 100.0, Some(1.0), 7.0, 200.0, true);

    let down = state.handle_action(&UiAction::new(POINTER_DOWN_ACTION).payload("107,12"));
    let unchanged = state.handle_action(&UiAction::new(POINTER_DRAG_ACTION).payload("107,20"));
    let drag = state.handle_action(&UiAction::new(POINTER_DRAG_ACTION).payload("207,20"));
    let release = state.handle_action(&UiAction::new(POINTER_UP_ACTION).payload("157,20"));

    assert!(down.handled && down.changed);
    assert_eq!(down.events[0].id().as_str(), CHANGE_EVENT);
    assert_eq!(down.events[0].payload_value(), Some("50"));
    assert!(unchanged.handled && !unchanged.changed);
    assert!(unchanged.events.is_empty());
    assert_eq!(drag.events[0].payload_value(), Some("100"));
    assert_eq!(release.events.len(), 2);
    assert_eq!(release.events[0].id().as_str(), CHANGE_EVENT);
    assert_eq!(release.events[0].payload_value(), Some("75"));
    assert_eq!(release.events[1].id().as_str(), COMMIT_EVENT);
    assert_eq!(release.events[1].payload_value(), Some("75"));
    assert!(same_value(state.display_value(), 75.0));
}

#[test]
fn invalid_ranges_values_and_steps_are_sanitized() {
    assert_eq!(normalize_range(5.0, -5.0), (-5.0, 5.0));
    assert_eq!(normalize_range(f64::NAN, f64::INFINITY), (0.0, 1.0));
    assert!(same_value(clamp_value(f64::NAN, 2.0, 4.0), 2.0));
    assert_eq!(valid_step(0.0), None);
    assert_eq!(valid_step(f64::NAN), None);
}
