use std::any::Any;

use crate::core::{
    ComponentActionOutcome, ComponentState, UiAction, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION,
    POINTER_UP_ACTION,
};

use super::{
    math::*,
    model::{CHANGE_EVENT, COMMIT_EVENT},
};

#[derive(Clone)]
pub(super) struct SliderState {
    pub(super) controlled_value: f64,
    pub(super) preview_value: Option<f64>,
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) step: Option<f64>,
    pub(super) pointer_start: f32,
    pub(super) pointer_width: f32,
    pub(super) enabled: bool,
    pub(super) dragging: bool,
}

impl Default for SliderState {
    fn default() -> Self {
        Self {
            controlled_value: 0.0,
            preview_value: None,
            min: 0.0,
            max: 1.0,
            step: None,
            pointer_start: 0.0,
            pointer_width: 1.0,
            enabled: false,
            dragging: false,
        }
    }
}

impl SliderState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn configure(
        &mut self,
        controlled_value: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
        pointer_start: f32,
        pointer_width: f32,
        enabled: bool,
    ) {
        if !same_value(self.controlled_value, controlled_value) {
            self.controlled_value = controlled_value;
            if self
                .preview_value
                .is_some_and(|preview| same_value(preview, controlled_value))
            {
                self.preview_value = None;
            }
        }
        self.min = min;
        self.max = max;
        self.step = step;
        self.pointer_start = pointer_start;
        self.pointer_width = pointer_width.max(1.0);
        self.enabled = enabled;
        if !enabled {
            self.preview_value = None;
            self.dragging = false;
        }
    }

    pub(super) fn display_value(&self) -> f64 {
        self.preview_value.unwrap_or(self.controlled_value)
    }

    fn update_from_pointer(&mut self, x: f32) -> Option<f64> {
        if !self.enabled {
            return None;
        }
        let value = value_from_pointer(
            x,
            self.pointer_start,
            self.pointer_width,
            self.min,
            self.max,
            self.step,
        );
        if self
            .preview_value
            .is_some_and(|preview| same_value(preview, value))
            || (self.preview_value.is_none() && same_value(self.controlled_value, value))
        {
            return None;
        }
        self.preview_value = Some(value);
        Some(value)
    }

    fn semantic_value(&mut self, value: f64) -> ComponentActionOutcome {
        if !self.enabled {
            return ComponentActionOutcome::ignored();
        }
        let value = quantize_value(value, self.min, self.max, self.step);
        if same_value(self.display_value(), value) {
            return ComponentActionOutcome::handled(false);
        }
        self.preview_value = Some(value);
        ComponentActionOutcome::handled(true)
            .emit(UiAction::new(CHANGE_EVENT).payload(value.to_string()))
            .emit(UiAction::new(COMMIT_EVENT).payload(value.to_string()))
    }
}

impl ComponentState for SliderState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
        match action.id().as_str() {
            "semantic.set_value" => action
                .payload_value()
                .and_then(|value| value.parse::<f64>().ok())
                .map_or_else(ComponentActionOutcome::ignored, |value| {
                    self.semantic_value(value)
                }),
            "semantic.increment" => self.semantic_value(
                self.display_value() + self.step.unwrap_or((self.max - self.min) / 100.0),
            ),
            "semantic.decrement" => self.semantic_value(
                self.display_value() - self.step.unwrap_or((self.max - self.min) / 100.0),
            ),
            POINTER_DOWN_ACTION if self.enabled => {
                self.dragging = true;
                let value = action
                    .payload_value()
                    .and_then(parse_pointer_x)
                    .and_then(|x| self.update_from_pointer(x));
                let mut outcome = ComponentActionOutcome::handled(true);
                if let Some(value) = value {
                    outcome = outcome.emit(UiAction::new(CHANGE_EVENT).payload(value.to_string()));
                }
                outcome
            }
            POINTER_DRAG_ACTION if self.dragging => {
                let value = action
                    .payload_value()
                    .and_then(parse_pointer_x)
                    .and_then(|x| self.update_from_pointer(x));
                value.map_or_else(
                    || ComponentActionOutcome::handled(false),
                    |value| {
                        ComponentActionOutcome::handled(true)
                            .emit(UiAction::new(CHANGE_EVENT).payload(value.to_string()))
                    },
                )
            }
            POINTER_UP_ACTION if self.dragging => {
                let value = action
                    .payload_value()
                    .and_then(parse_pointer_x)
                    .and_then(|x| self.update_from_pointer(x));
                self.dragging = false;
                let mut outcome = ComponentActionOutcome::handled(true);
                if let Some(value) = value {
                    outcome = outcome.emit(UiAction::new(CHANGE_EVENT).payload(value.to_string()));
                }
                outcome.emit(UiAction::new(COMMIT_EVENT).payload(self.display_value().to_string()))
            }
            _ => ComponentActionOutcome::ignored(),
        }
    }
}
