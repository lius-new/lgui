use std::sync::Arc;

use crate::core::{
    AnimProperty, AnimationBinding, ElementRenderCx, InteractionRole, SemanticAction, SemanticRole,
    Semantics, Stroke, TextStyle, UiElement, UiId, UiRect, VisualStyle,
};
use crate::theme::ThemeContext;

use super::{math::*, model::*, state::SliderState};

pub(super) fn render_slider(cx: ElementRenderCx<'_, '_, '_>, slider: Slider) -> UiElement {
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
    let enabled = slider.enabled && slider.max > slider.min && geometry.track.width() > 0.0;
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
    let thumb_x = geometry.track.left + geometry.track.width() * progress as f32;
    let hover = smootherstep(cx.animation_value(AnimProperty::Hover));
    let pressed = smootherstep(cx.animation_value(AnimProperty::Pressed));
    let thumb_size = style.thumb_size.max(6.0) + pressed * 2.0;
    let thumb_radius = thumb_size / 2.0;
    let center_y = (slider.rect.top + slider.rect.bottom) / 2.0;
    let track_color = mix_color(style.track, style.active_track, hover * 0.18);
    let thumb_color = mix_color(style.thumb, style.value_text, hover * 0.28 + pressed * 0.22);
    let alpha = if enabled { 0xFF } else { style.disabled_alpha };

    let mut root = UiElement::panel(id.clone(), slider.rect, VisualStyle::default())
        .render_phase(slider.phase)
        .paint_bounds(slider.rect)
        .semantics({
            let mut semantics = Semantics::new(SemanticRole::Slider)
                .value(display_value.to_string())
                .action(SemanticAction::Focus)
                .action(SemanticAction::SetValue)
                .action(SemanticAction::Increment)
                .action(SemanticAction::Decrement);
            semantics.numeric_value = Some(display_value);
            semantics.numeric_min = Some(slider.min);
            semantics.numeric_max = Some(slider.max);
            semantics.numeric_step = slider.step;
            semantics.state.disabled = !enabled;
            semantics
        })
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
                    .radius(style.track_height.max(1.0) / 2.0),
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
                    .radius(style.track_height.max(1.0) / 2.0),
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
                    .stroked(Stroke::new(style.thumb_border, 2.0, alpha))
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
                TextStyle::new(style.value_text, -15.0, 700).centered(),
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
    let thumb_radius = (style.thumb_size.max(6.0) + 2.0) / 2.0;
    let label_width = SLIDER_VALUE_WIDTH.min(rect.width().max(0.0));
    let label_left = rect.right - label_width;
    let content_right = if has_value_text {
        (label_left - SLIDER_VALUE_GAP).max(rect.left)
    } else {
        rect.right
    };
    let center_y = (rect.top + rect.bottom) / 2.0;
    let track_height = style.track_height.max(1.0).min(rect.height().max(1.0));
    let track_left = (rect.left + thumb_radius).min(content_right);
    let track_right = (content_right - thumb_radius).max(track_left);
    let track = UiRect::new(
        track_left,
        center_y - track_height / 2.0,
        track_right,
        center_y - track_height / 2.0 + track_height,
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
