use std::{any::Any, ops::RangeInclusive, sync::Arc};

use crate::core::{
    AnimProperty, AnimationBinding, Color, ComponentActionOutcome, ComponentState, Element,
    ElementKey, ElementRenderCx, InteractionRole, RenderPhase, Stroke, TextStyle, UiAction,
    UiElement, UiEventContext, UiId, UiRect, VisualStyle, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION,
    POINTER_UP_ACTION,
};
use crate::theme::{ThemeContext, ThemeTokens};

pub type SliderChangeHandler = Arc<dyn Fn(&mut UiEventContext, f64) + Send + Sync>;
pub type SliderValueFormatter = Arc<dyn Fn(f64) -> String + Send + Sync>;
const SLIDER_HOVER_IN_MS: f32 = 110.0;
const SLIDER_HOVER_OUT_MS: f32 = 150.0;
const SLIDER_VALUE_WIDTH: i32 = 48;
const SLIDER_VALUE_GAP: i32 = 10;
const CHANGE_EVENT: &str = "slider.change";
const COMMIT_EVENT: &str = "slider.commit";

pub trait IntoSliderChangeHandler {
    fn into_slider_change_handler(self) -> SliderChangeHandler;
}

impl<F> IntoSliderChangeHandler for F
where
    F: Fn(f64) + Send + Sync + 'static,
{
    fn into_slider_change_handler(self) -> SliderChangeHandler {
        Arc::new(move |_context, value| self(value))
    }
}

impl IntoSliderChangeHandler for SliderChangeHandler {
    fn into_slider_change_handler(self) -> SliderChangeHandler {
        self
    }
}

#[derive(Clone, Copy)]
pub struct SliderStyle {
    pub track: Color,
    pub active_track: Color,
    pub thumb: Color,
    pub thumb_border: Color,
    pub value_text: Color,
    pub track_height: i32,
    pub thumb_size: i32,
    pub disabled_alpha: u8,
}

impl SliderStyle {
    pub fn from_theme(theme: &ThemeTokens) -> Self {
        Self {
            track: theme.colors.border,
            active_track: theme.colors.accent,
            thumb: theme.colors.accent,
            thumb_border: theme.colors.surface_raised,
            value_text: theme.colors.accent,
            track_height: 4,
            thumb_size: 14,
            disabled_alpha: 0x60,
        }
    }
}

impl Default for SliderStyle {
    fn default() -> Self {
        Self::from_theme(&ThemeTokens::default())
    }
}

pub struct Slider {
    key: ElementKey,
    rect: UiRect,
    value: f64,
    min: f64,
    max: f64,
    step: Option<f64>,
    enabled: bool,
    phase: RenderPhase,
    style: Option<SliderStyle>,
    value_formatter: Option<SliderValueFormatter>,
    on_change: SliderChangeHandler,
    on_commit: Option<SliderChangeHandler>,
}

impl Slider {
    pub fn step(mut self, step: f64) -> Self {
        self.step = valid_step(step);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
        self
    }

    pub fn style(mut self, style: SliderStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn format_value(
        mut self,
        formatter: impl Fn(f64) -> String + Send + Sync + 'static,
    ) -> Self {
        self.value_formatter = Some(Arc::new(formatter));
        self
    }

    pub fn on_commit(mut self, on_commit: impl IntoSliderChangeHandler) -> Self {
        self.on_commit = Some(on_commit.into_slider_change_handler());
        self
    }
}

impl From<Slider> for Element {
    fn from(value: Slider) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            render_slider(cx, value)
        })
    }
}

#[track_caller]
pub fn slider(
    rect: UiRect,
    value: f64,
    range: RangeInclusive<f64>,
    on_change: impl IntoSliderChangeHandler,
) -> Slider {
    let (min, max) = normalize_range(*range.start(), *range.end());
    Slider {
        key: ElementKey::caller(),
        rect,
        value,
        min,
        max,
        step: None,
        enabled: true,
        phase: RenderPhase::Content,
        style: None,
        value_formatter: None,
        on_change: on_change.into_slider_change_handler(),
        on_commit: None,
    }
}

fn render_slider(cx: ElementRenderCx<'_, '_, '_>, slider: Slider) -> UiElement {
    let id = cx.id.clone();
    let on_change = Arc::clone(&slider.on_change);
    let on_commit = slider.on_commit.clone();
    let style = slider.style.unwrap_or_else(|| {
        cx.try_use_context::<ThemeContext>()
            .map(|theme| SliderStyle::from_theme(theme.tokens()))
            .unwrap_or_default()
    });
    let geometry = slider_geometry(slider.rect, slider.value_formatter.is_some(), style);
    let controlled_value = clamp_value(slider.value, slider.min, slider.max);
    let enabled = slider.enabled && slider.max > slider.min && geometry.track.width() > 0;
    let display_value = cx
        .context
        .component_state_mut(&id, |state: &mut SliderState| {
            state.configure(
                controlled_value,
                slider.min,
                slider.max,
                slider.step,
                geometry.track.left - geometry.hit_rect.left,
                geometry.track.width(),
                enabled,
            );
            state.display_value()
        });
    let progress = normalized_progress(display_value, slider.min, slider.max);
    let thumb_x = geometry.track.left + (geometry.track.width() as f64 * progress).round() as i32;
    let hover = smootherstep(cx.animation_value(AnimProperty::Hover));
    let pressed = smootherstep(cx.animation_value(AnimProperty::Pressed));
    let thumb_size = style.thumb_size.max(6) + (pressed * 2.0).round() as i32;
    let thumb_radius = thumb_size / 2;
    let center_y = (slider.rect.top + slider.rect.bottom) / 2;
    let track_color = mix_color(style.track, style.active_track, hover * 0.18);
    let thumb_color = mix_color(style.thumb, style.value_text, hover * 0.28 + pressed * 0.22);
    let alpha = if enabled { 0xFF } else { style.disabled_alpha };

    let mut root = UiElement::panel(id.clone(), slider.rect, VisualStyle::default())
        .render_phase(slider.phase)
        .paint_bounds(slider.rect)
        .animation(
            AnimationBinding::new(AnimProperty::Hover, 0.0, 1.0)
                .duration(SLIDER_HOVER_IN_MS, SLIDER_HOVER_OUT_MS),
        )
        .animation(
            AnimationBinding::new(AnimProperty::Pressed, 0.0, 1.0)
                .duration(SLIDER_HOVER_IN_MS, SLIDER_HOVER_OUT_MS),
        )
        .on_action(CHANGE_EVENT, move |context, action| {
            let Some(value) = action
                .payload_value()
                .and_then(|payload| payload.parse::<f64>().ok())
            else {
                return;
            };
            on_change(context, value);
        })
        .child(
            UiElement::panel(
                UiId::owned(format!("{}.track", id.as_str())),
                geometry.track,
                VisualStyle::filled(track_color)
                    .alpha(mix_u8(0x90, 0xB8, hover).min(alpha))
                    .radius(style.track_height.max(1) / 2),
            )
            .render_phase(slider.phase),
        )
        .child(
            UiElement::panel(
                UiId::owned(format!("{}.active", id.as_str())),
                UiRect::new(
                    geometry.track.left,
                    geometry.track.top,
                    thumb_x,
                    geometry.track.bottom,
                ),
                VisualStyle::filled(style.active_track)
                    .alpha(alpha)
                    .radius(style.track_height.max(1) / 2),
            )
            .render_phase(slider.phase),
        )
        .child(
            UiElement::panel(
                UiId::owned(format!("{}.thumb", id.as_str())),
                UiRect::new(
                    thumb_x - thumb_radius,
                    center_y - thumb_radius,
                    thumb_x - thumb_radius + thumb_size,
                    center_y - thumb_radius + thumb_size,
                ),
                VisualStyle::filled(thumb_color)
                    .alpha(alpha)
                    .stroked(Stroke::new(style.thumb_border, 2, alpha))
                    .radius(thumb_radius),
            )
            .render_phase(slider.phase),
        );

    if let Some(on_commit) = on_commit {
        root = root.on_action(COMMIT_EVENT, move |context, action| {
            let Some(value) = action
                .payload_value()
                .and_then(|payload| payload.parse::<f64>().ok())
            else {
                return;
            };
            on_commit(context, value);
        });
    }

    if enabled {
        root = root
            .hit_rect(geometry.hit_rect)
            .interaction(InteractionRole::Button);
    }
    if let (Some(label_rect), Some(formatter)) = (geometry.label_rect, slider.value_formatter) {
        root = root.child(
            UiElement::text(
                UiId::owned(format!("{}.value", id.as_str())),
                label_rect,
                formatter(display_value),
                TextStyle::new(style.value_text, -15, 700).centered(),
            )
            .render_phase(slider.phase),
        );
    }
    root
}

#[derive(Clone, Copy)]
struct SliderGeometry {
    track: UiRect,
    hit_rect: UiRect,
    label_rect: Option<UiRect>,
}

fn slider_geometry(rect: UiRect, has_value_text: bool, style: SliderStyle) -> SliderGeometry {
    let thumb_radius = (style.thumb_size.max(6) + 2) / 2;
    let label_width = SLIDER_VALUE_WIDTH.min(rect.width().max(0));
    let label_left = rect.right - label_width;
    let content_right = if has_value_text {
        (label_left - SLIDER_VALUE_GAP).max(rect.left)
    } else {
        rect.right
    };
    let center_y = (rect.top + rect.bottom) / 2;
    let track_height = style.track_height.max(1).min(rect.height().max(1));
    let track_left = (rect.left + thumb_radius).min(content_right);
    let track_right = (content_right - thumb_radius).max(track_left);
    let track = UiRect::new(
        track_left,
        center_y - track_height / 2,
        track_right,
        center_y - track_height / 2 + track_height,
    );
    SliderGeometry {
        track,
        hit_rect: UiRect::new(
            (track.left - thumb_radius).max(rect.left),
            rect.top,
            (track.right + thumb_radius).min(content_right),
            rect.bottom,
        ),
        label_rect: has_value_text.then_some(UiRect::new(
            label_left,
            rect.top,
            rect.right,
            rect.bottom,
        )),
    }
}

#[derive(Clone)]
struct SliderState {
    controlled_value: f64,
    preview_value: Option<f64>,
    min: f64,
    max: f64,
    step: Option<f64>,
    pointer_start: i32,
    pointer_width: i32,
    enabled: bool,
    dragging: bool,
}

impl Default for SliderState {
    fn default() -> Self {
        Self {
            controlled_value: 0.0,
            preview_value: None,
            min: 0.0,
            max: 1.0,
            step: None,
            pointer_start: 0,
            pointer_width: 1,
            enabled: false,
            dragging: false,
        }
    }
}

impl SliderState {
    #[allow(clippy::too_many_arguments)]
    fn configure(
        &mut self,
        controlled_value: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
        pointer_start: i32,
        pointer_width: i32,
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
        self.pointer_width = pointer_width.max(1);
        self.enabled = enabled;
        if !enabled {
            self.preview_value = None;
            self.dragging = false;
        }
    }

    fn display_value(&self) -> f64 {
        self.preview_value.unwrap_or(self.controlled_value)
    }

    fn update_from_pointer(&mut self, x: i32) -> Option<f64> {
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

fn parse_pointer_x(payload: &str) -> Option<i32> {
    payload
        .split_once(',')
        .and_then(|(x, _)| x.parse::<i32>().ok())
}

fn value_from_pointer(
    x: i32,
    pointer_start: i32,
    pointer_width: i32,
    min: f64,
    max: f64,
    step: Option<f64>,
) -> f64 {
    let offset = (x - pointer_start).clamp(0, pointer_width.max(1));
    let progress = offset as f64 / pointer_width.max(1) as f64;
    quantize_value(min + (max - min) * progress, min, max, step)
}

fn quantize_value(value: f64, min: f64, max: f64, step: Option<f64>) -> f64 {
    let value = clamp_value(value, min, max);
    let Some(step) = step else {
        return value;
    };
    clamp_value(min + ((value - min) / step).round() * step, min, max)
}

fn normalized_progress(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        0.0
    } else {
        ((clamp_value(value, min, max) - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn normalize_range(start: f64, end: f64) -> (f64, f64) {
    let start = if start.is_finite() { start } else { 0.0 };
    let end = if end.is_finite() { end } else { 1.0 };
    if start <= end {
        (start, end)
    } else {
        (end, start)
    }
}

fn clamp_value(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

fn valid_step(step: f64) -> Option<f64> {
    (step.is_finite() && step > 0.0).then_some(step)
}

fn same_value(left: f64, right: f64) -> bool {
    (left - right).abs() <= f64::EPSILON * left.abs().max(right.abs()).max(1.0) * 4.0
}

fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn mix_color(from: Color, to: Color, value: f32) -> Color {
    let value = value.clamp(0.0, 1.0);
    let from = from.0;
    let to = to.0;
    let fr = ((from >> 16) & 0xFF) as f32;
    let fg = ((from >> 8) & 0xFF) as f32;
    let fb = (from & 0xFF) as f32;
    let tr = ((to >> 16) & 0xFF) as f32;
    let tg = ((to >> 8) & 0xFF) as f32;
    let tb = (to & 0xFF) as f32;
    let r = (fr + (tr - fr) * value).round() as u32;
    let g = (fg + (tg - fg) * value).round() as u32;
    let b = (fb + (tb - fb) * value).round() as u32;
    Color((r << 16) | (g << 8) | b)
}

fn mix_u8(from: u8, to: u8, value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    (from as f32 + (to as f32 - from as f32) * value).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_position_maps_to_continuous_values_and_clamps() {
        assert!(same_value(
            value_from_pointer(50, 0, 200, -1.0, 1.0, None),
            -0.5
        ));
        assert!(same_value(
            value_from_pointer(-20, 0, 200, -1.0, 1.0, None),
            -1.0
        ));
        assert!(same_value(
            value_from_pointer(240, 0, 200, -1.0, 1.0, None),
            1.0
        ));
    }

    #[test]
    fn optional_step_quantizes_relative_to_the_minimum() {
        assert!(same_value(
            value_from_pointer(44, 0, 100, 10.0, 20.0, Some(0.5)),
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
        state.configure(20.0, 0.0, 100.0, Some(1.0), 7, 200, true);

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
}
