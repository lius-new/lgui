use super::{
    model::*,
    render::{mix_color, mix_u8, select_menu_height, select_menu_layout, smootherstep},
    state::SelectState,
};
use crate::core::{Color, ComponentState, UiAction, UiRect};

#[test]
fn select_state_owns_open_state_and_reports_controlled_changes() {
    let mut state = SelectState::default();
    state.configure(0, 3, 3, true);

    let toggle = state.handle_action(&UiAction::new(TOGGLE_ACTION));
    assert!(toggle.handled);
    assert!(toggle.changed);
    assert!(state.open);
    let selection = state.handle_action(&UiAction::new(SELECT_ACTION).payload("2"));
    assert!(selection.handled);
    assert!(selection.changed);
    assert!(!state.open);
    assert_eq!(state.selected, 2);
    assert_eq!(selection.events.len(), 1);
    assert_eq!(selection.events[0].id().as_str(), CHANGE_EVENT);
    assert_eq!(selection.events[0].payload_value(), Some("2"));
}

#[test]
fn selecting_the_controlled_value_only_closes_the_menu() {
    let mut state = SelectState::default();
    state.configure(1, 2, 2, true);
    state.open = true;

    let selection = state.handle_action(&UiAction::new(SELECT_ACTION).payload("1"));
    assert!(selection.handled);
    assert!(selection.changed);
    assert!(!state.open);
    assert!(selection.events.is_empty());
}

#[test]
fn invalid_or_disabled_actions_are_ignored() {
    let mut state = SelectState::default();
    state.configure(0, 1, 1, false);

    assert!(!state.handle_action(&UiAction::new(TOGGLE_ACTION)).handled);
    assert!(
        !state
            .handle_action(&UiAction::new(SELECT_ACTION).payload("4"))
            .handled
    );
    assert!(!state.open);
}

#[test]
fn long_select_scrolls_by_rows_and_clamps_to_its_bounds() {
    let mut state = SelectState::default();
    state.configure(0, 100, 8, true);
    state.handle_action(&UiAction::new(TOGGLE_ACTION));

    assert!(
        state
            .handle_action(&UiAction::new(SCROLL_ACTION).payload("1"))
            .changed
    );
    assert_eq!(state.scroll_start, 3);
    assert!(
        state
            .handle_action(&UiAction::new(SCROLL_ACTION).payload("100"))
            .changed
    );
    assert_eq!(state.scroll_start, 92);
    assert!(
        !state
            .handle_action(&UiAction::new(SCROLL_ACTION).payload("1"))
            .changed
    );
    assert!(
        state
            .handle_action(&UiAction::new(SCROLL_ACTION).payload("-100"))
            .changed
    );
    assert_eq!(state.scroll_start, 0);
}

#[test]
fn controlled_selection_is_kept_inside_the_visible_window() {
    let mut state = SelectState::default();
    state.configure(75, 100, 8, true);

    assert_eq!(state.scroll_start, 68);
    state.configure(2, 100, 8, true);
    assert_eq!(state.scroll_start, 2);
}

#[test]
fn auto_placement_opens_above_when_the_menu_does_not_fit_below() {
    let viewport = UiRect::new(0.0, 0.0, 800.0, 700.0);
    let anchor = UiRect::new(500.0, 600.0, 720.0, 638.0);
    let menu = select_menu_layout(anchor, 100, viewport, SelectPlacement::Auto);

    assert_eq!(menu.placement, SelectPlacement::Above);
    assert_eq!(menu.visible_count, 8);
    assert_eq!(menu.rect.bottom, anchor.top - SELECT_MENU_GAP);
    assert_eq!(menu.rect.height(), select_menu_height(8));
}

#[test]
fn auto_placement_prefers_below_when_the_full_menu_fits() {
    let viewport = UiRect::new(0.0, 0.0, 800.0, 700.0);
    let anchor = UiRect::new(500.0, 100.0, 720.0, 138.0);
    let menu = select_menu_layout(anchor, 100, viewport, SelectPlacement::Auto);

    assert_eq!(menu.placement, SelectPlacement::Below);
    assert_eq!(menu.visible_count, 8);
    assert_eq!(menu.rect.top, anchor.bottom + SELECT_MENU_GAP);
}

#[test]
fn explicit_placement_is_respected_and_reduces_visible_rows_to_fit() {
    let viewport = UiRect::new(0.0, 0.0, 800.0, 700.0);
    let anchor = UiRect::new(500.0, 600.0, 720.0, 638.0);
    let menu = select_menu_layout(anchor, 100, viewport, SelectPlacement::Below);

    assert_eq!(menu.placement, SelectPlacement::Below);
    assert_eq!(menu.visible_count, 1);
    assert!(menu.rect.bottom <= viewport.bottom);
}

#[test]
fn select_hover_visuals_interpolate_between_stable_endpoints() {
    assert_eq!(mix_u8(0x20, 0x80, 0.0), 0x20);
    assert_eq!(mix_u8(0x20, 0x80, 1.0), 0x80);
    assert_eq!(
        mix_color(Color(0x000000), Color(0xFFFFFF), 0.5),
        Color(0x808080)
    );
    assert_eq!(smootherstep(0.0), 0.0);
    assert_eq!(smootherstep(1.0), 1.0);
}
