use std::any::Any;

use crate::core::{ComponentActionOutcome, ComponentState, UiAction};

use super::model::{CHANGE_EVENT, SCROLL_ACTION, SELECT_ACTION, SELECT_WHEEL_ROWS, TOGGLE_ACTION};

#[derive(Clone)]
pub(super) struct SelectState {
    pub(super) open: bool,
    pub(super) selected: usize,
    pub(super) option_count: usize,
    pub(super) visible_count: usize,
    pub(super) scroll_start: usize,
    pub(super) enabled: bool,
}

impl Default for SelectState {
    fn default() -> Self {
        Self {
            open: false,
            selected: 0,
            option_count: 0,
            visible_count: 0,
            scroll_start: 0,
            enabled: false,
        }
    }
}

impl SelectState {
    pub(super) fn configure(
        &mut self,
        selected: usize,
        option_count: usize,
        visible_count: usize,
        enabled: bool,
    ) {
        let selected_changed = self.selected != selected;
        self.selected = selected;
        self.option_count = option_count;
        self.visible_count = visible_count.min(option_count);
        self.enabled = enabled;
        self.scroll_start = self.scroll_start.min(self.max_scroll_start());
        if selected_changed {
            self.ensure_selected_visible();
        }
        if !enabled {
            self.open = false;
        }
    }

    fn max_scroll_start(&self) -> usize {
        self.option_count.saturating_sub(self.visible_count)
    }

    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll_start {
            self.scroll_start = self.selected;
        } else if self.selected >= self.scroll_start.saturating_add(self.visible_count) {
            self.scroll_start = self
                .selected
                .saturating_add(1)
                .saturating_sub(self.visible_count);
        }
        self.scroll_start = self.scroll_start.min(self.max_scroll_start());
    }

    fn scroll(&mut self, wheel_units: i32) -> bool {
        if wheel_units == 0 || self.option_count <= self.visible_count {
            return false;
        }
        let delta = wheel_units.saturating_mul(SELECT_WHEEL_ROWS);
        let next = if delta > 0 {
            self.scroll_start.saturating_add(delta as usize)
        } else {
            self.scroll_start
                .saturating_sub(delta.saturating_abs() as usize)
        }
        .min(self.max_scroll_start());
        if next == self.scroll_start {
            return false;
        }
        self.scroll_start = next;
        true
    }
}

impl ComponentState for SelectState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
        match action.id().as_str() {
            TOGGLE_ACTION if self.enabled => {
                self.open = !self.open;
                if self.open {
                    self.ensure_selected_visible();
                }
                ComponentActionOutcome::handled(true)
            }
            SELECT_ACTION if self.enabled => {
                let Some(index) = action
                    .payload_value()
                    .and_then(|payload| payload.parse::<usize>().ok())
                    .filter(|index| *index < self.option_count)
                else {
                    return ComponentActionOutcome::ignored();
                };
                self.open = false;
                let changed = index != self.selected;
                self.selected = index;
                if changed {
                    ComponentActionOutcome::handled(true)
                        .emit(UiAction::new(CHANGE_EVENT).payload(index.to_string()))
                } else {
                    ComponentActionOutcome::handled(true)
                }
            }
            SCROLL_ACTION if self.enabled && self.open => {
                let changed = action
                    .payload_value()
                    .and_then(|payload| payload.parse::<i32>().ok())
                    .is_some_and(|wheel_units| self.scroll(wheel_units));
                changed.into()
            }
            _ => ComponentActionOutcome::ignored(),
        }
    }
}
