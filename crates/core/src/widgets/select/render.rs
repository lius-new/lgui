use std::sync::Arc;

use crate::core::{
    AnimProperty, AnimationBinding, Color, ElementRenderCx, EventPolicy, IconStyle,
    InteractionRole, RenderPhase, SemanticAction, SemanticRole, Semantics, Stroke, TextStyle,
    UiAction, UiElement, UiId, UiRect, UiRenderContext, VisualStyle,
};
use crate::theme::ThemeContext;

use super::{model::*, state::SelectState};

pub(super) fn render_select(cx: ElementRenderCx<'_, '_, '_>, select: Select) -> UiElement {
    let id = cx.id.clone();
    let on_change = Arc::clone(&select.on_change);
    let style = select.style.unwrap_or_else(|| {
        cx.try_use_context::<ThemeContext>()
            .map(|theme| SelectStyle::from_theme(theme.tokens()))
            .unwrap_or_default()
    });
    let hover = smootherstep(cx.animation_value(AnimProperty::Hover));
    let enabled = select.enabled && !select.options.is_empty();
    let selected = normalize_selected(select.selected, select.options.len());
    let menu_layout = select_menu_layout(
        select.rect,
        select.options.len(),
        cx.context.viewport(),
        select.placement,
    );
    let focused = cx.context.interaction_flags(&id).focused;
    let (open, scroll_start) = cx
        .context
        .component_state_mut(&id, |state: &mut SelectState| {
            state.configure(
                selected,
                select.options.len(),
                menu_layout.visible_count,
                enabled,
            );
            if state.open && !focused {
                state.open = false;
            }
            (state.open, state.scroll_start)
        });
    let menu_rect = menu_layout.rect;
    let paint_bounds = select.rect.union(menu_rect);
    let control_fill = mix_color(style.control_fill, style.control_hover_fill, hover * 0.42);
    let control_border = if open {
        style.open_border
    } else {
        mix_color(style.border, style.open_border, hover * 0.28)
    };
    let border_alpha = if open {
        0xD8
    } else {
        mix_u8(0x58, 0x88, hover)
    };
    let control_text_left = if select
        .options
        .get(selected)
        .and_then(|option| option.swatch)
        .is_some()
    {
        select.rect.left + 40.0
    } else {
        select.rect.left + 12.0
    };

    let mut root = UiElement::panel(
        id.clone(),
        select.rect,
        VisualStyle::filled(control_fill)
            .alpha(0xF2)
            .stroked(Stroke::new(control_border, 1.0, border_alpha))
            .radius(style.radius),
    )
    .render_phase(select.phase)
    .paint_bounds(paint_bounds)
    .semantics({
        let mut semantics = Semantics::new(SemanticRole::ComboBox)
            .action(SemanticAction::Focus)
            .action(SemanticAction::Click);
        if let Some(option) = select.options.get(selected) {
            semantics.value = Some(option.label.to_string());
        }
        semantics.state.disabled = !enabled;
        semantics.state.expanded = Some(open);
        semantics
    })
    .on_action(CHANGE_EVENT, move |context, action| {
        let Some(index) = action
            .payload_value()
            .and_then(|payload| payload.parse::<usize>().ok())
        else {
            return;
        };
        on_change(context, index);
    })
    .animation(
        AnimationBinding::new(AnimProperty::Hover, 0.0, 1.0)
            .duration(SELECT_HOVER_IN_MS, SELECT_HOVER_OUT_MS),
    );

    if enabled {
        root = root
            .interaction(InteractionRole::Button)
            .click_action(UiAction::new(TOGGLE_ACTION));
    }

    if let Some(option) = select.options.get(selected) {
        if let Some(swatch) = option.swatch {
            root = root.child(render_swatch(
                UiId::owned(format!("{}.selected.swatch", id.as_str())),
                UiRect::new(
                    select.rect.left + 12.0,
                    select.rect.top + 10.0,
                    select.rect.left + 30.0,
                    select.rect.bottom - 10.0,
                ),
                swatch,
                style.swatch_border,
                select.phase,
            ));
        }
        root = root.child(
            UiElement::text(
                UiId::owned(format!("{}.selected.label", id.as_str())),
                UiRect::new(
                    control_text_left,
                    select.rect.top,
                    select.rect.right - 34.0,
                    select.rect.bottom,
                ),
                option.label.clone(),
                TextStyle::new(style.selected_text, -14.0, 500),
            )
            .render_phase(select.phase),
        );
    }

    root = root.child(
        UiElement::icon(
            UiId::owned(format!("{}.chevron", id.as_str())),
            UiRect::new(
                select.rect.right - 28.0,
                select.rect.top + (select.rect.height() - 16.0) / 2.0,
                select.rect.right - 12.0,
                select.rect.top + (select.rect.height() + 16.0) / 2.0,
            ),
            if open { "chevron-up" } else { "chevron-down" },
        )
        .icon_style(IconStyle::new(if open {
            style.active_icon
        } else {
            mix_color(style.icon, style.selected_text, hover)
        }))
        .render_phase(select.phase),
    );

    if open {
        root = root.child(render_menu(
            &id,
            cx.context,
            menu_rect,
            selected,
            &select.options,
            menu_layout.visible_count,
            scroll_start,
            style,
            RenderPhase::Popup,
        ));
    }
    root
}

fn render_menu(
    select_id: &UiId,
    context: &UiRenderContext<'_>,
    rect: UiRect,
    selected: usize,
    options: &[SelectOption],
    visible_count: usize,
    scroll_start: usize,
    style: SelectStyle,
    phase: RenderPhase,
) -> UiElement {
    let mut menu = UiElement::panel(
        UiId::owned(format!("{}.menu", select_id.as_str())),
        rect,
        VisualStyle::filled(style.menu_fill)
            .alpha(0xFA)
            .stroked(Stroke::new(style.border, 1.0, 0x70))
            .radius(style.radius),
    )
    .render_phase(phase);
    if options.len() > visible_count {
        menu = menu
            .wheel_action(UiAction::new(SCROLL_ACTION))
            .action_target(select_id.clone());
    }

    for (slot, (index, option)) in options
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_count)
        .enumerate()
    {
        let top = rect.top + SELECT_MENU_PADDING + slot as f32 * SELECT_OPTION_HEIGHT;
        let item = UiRect::new(
            rect.left + SELECT_MENU_PADDING,
            top,
            rect.right - SELECT_MENU_PADDING,
            top + SELECT_OPTION_HEIGHT,
        );
        let option_id = UiId::owned(format!("{}.option.{index}", select_id.as_str()));
        let option_hover = smootherstep(context.animation_value(&option_id, AnimProperty::Hover));
        let is_selected = index == selected;
        let text_left = if option.swatch.is_some() {
            item.left + 38.0
        } else {
            item.left + 10.0
        };
        let text_right = if is_selected {
            item.right - 36.0
        } else {
            item.right - 10.0
        };
        let option_fill = if is_selected {
            style.selected_fill
        } else {
            style.option_hover_fill
        };
        let option_fill_alpha = if is_selected {
            mix_u8(0x72, 0xA0, option_hover)
        } else {
            mix_u8(0x00, 0x96, option_hover)
        };
        let mut option_element = UiElement::panel(
            option_id,
            item,
            VisualStyle::filled(option_fill)
                .alpha(option_fill_alpha)
                .radius((style.radius - 2.0).max(2.0)),
        )
        .render_phase(phase)
        .interaction(InteractionRole::Button)
        .semantics({
            let mut semantics = Semantics::new(SemanticRole::ListBoxOption)
                .name(option.label.to_string())
                .action(SemanticAction::Click);
            semantics.state.selected = is_selected;
            semantics
        })
        .event_policy(EventPolicy {
            hover: true,
            press: true,
            focus: false,
        })
        .click_action(UiAction::new(SELECT_ACTION).payload(index.to_string()))
        .action_target(select_id.clone())
        .animation(
            AnimationBinding::new(AnimProperty::Hover, 0.0, 1.0)
                .duration(SELECT_HOVER_IN_MS, SELECT_HOVER_OUT_MS),
        );

        if is_selected {
            option_element = option_element
                .child(
                    UiElement::panel(
                        UiId::owned(format!("{}.option.{index}.indicator", select_id.as_str())),
                        UiRect::new(
                            item.left + 2.0,
                            item.top + 10.0,
                            item.left + 4.0,
                            item.bottom - 10.0,
                        ),
                        VisualStyle::filled(style.active_icon).radius(1.0),
                    )
                    .render_phase(phase),
                )
                .child(
                    UiElement::icon(
                        UiId::owned(format!("{}.option.{index}.check", select_id.as_str())),
                        UiRect::new(
                            item.right - 27.0,
                            item.top + 11.0,
                            item.right - 11.0,
                            item.bottom - 11.0,
                        ),
                        "check",
                    )
                    .icon_style(IconStyle::new(style.active_icon))
                    .render_phase(phase),
                );
        }

        if let Some(swatch) = option.swatch {
            option_element = option_element.child(render_swatch(
                UiId::owned(format!("{}.option.{index}.swatch", select_id.as_str())),
                UiRect::new(
                    item.left + 10.0,
                    item.top + 11.0,
                    item.left + 28.0,
                    item.bottom - 11.0,
                ),
                swatch,
                style.swatch_border,
                phase,
            ));
        }
        option_element = option_element.child(
            UiElement::text(
                UiId::owned(format!("{}.option.{index}.label", select_id.as_str())),
                UiRect::new(text_left, item.top, text_right, item.bottom),
                option.label.clone(),
                TextStyle::new(
                    if is_selected {
                        style.selected_text
                    } else {
                        mix_color(style.text, style.selected_text, option_hover)
                    },
                    -14.0,
                    500,
                ),
            )
            .render_phase(phase),
        );
        menu = menu.child(option_element);
    }
    if options.len() > visible_count {
        menu = menu.child(render_scrollbar(
            select_id,
            rect,
            options.len(),
            visible_count,
            scroll_start,
            style,
            phase,
        ));
    }
    menu
}

fn render_scrollbar(
    select_id: &UiId,
    rect: UiRect,
    option_count: usize,
    visible_count: usize,
    scroll_start: usize,
    style: SelectStyle,
    phase: RenderPhase,
) -> UiElement {
    let track = UiRect::new(
        rect.right - 6.0,
        rect.top + 8.0,
        rect.right - 3.0,
        rect.bottom - 8.0,
    );
    let thumb_height = (track.height() * visible_count as f32 / option_count as f32)
        .max(24.0)
        .min(track.height());
    let max_start = option_count.saturating_sub(visible_count);
    let thumb_travel = track.height() - thumb_height;
    let thumb_top = if max_start == 0 {
        track.top
    } else {
        track.top + thumb_travel * scroll_start as f32 / max_start as f32
    };

    UiElement::panel(
        UiId::owned(format!("{}.scrollbar.track", select_id.as_str())),
        track,
        VisualStyle::filled(style.border).alpha(0x38).radius(2.0),
    )
    .render_phase(phase)
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.scrollbar.thumb", select_id.as_str())),
            UiRect::new(track.left, thumb_top, track.right, thumb_top + thumb_height),
            VisualStyle::filled(style.icon).alpha(0x92).radius(2.0),
        )
        .render_phase(phase),
    )
}

fn render_swatch(
    id: UiId,
    rect: UiRect,
    swatch: SelectSwatch,
    border: Color,
    phase: RenderPhase,
) -> UiElement {
    let center = (rect.left + rect.right) / 2.0;
    UiElement::panel(
        id.clone(),
        rect,
        VisualStyle::default()
            .stroked(Stroke::new(border, 1.0, 0xFF))
            .radius(2.0),
    )
    .render_phase(phase)
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.primary", id.as_str())),
            UiRect::new(rect.left + 1.0, rect.top + 1.0, center, rect.bottom - 1.0),
            VisualStyle::filled(swatch.primary),
        )
        .render_phase(phase),
    )
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.secondary", id.as_str())),
            UiRect::new(center, rect.top + 1.0, rect.right - 1.0, rect.bottom - 1.0),
            VisualStyle::filled(swatch.secondary),
        )
        .render_phase(phase),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SelectMenuLayout {
    pub(super) rect: UiRect,
    pub(super) visible_count: usize,
    pub(super) placement: SelectPlacement,
}

pub(super) fn select_menu_layout(
    anchor: UiRect,
    option_count: usize,
    viewport: UiRect,
    placement: SelectPlacement,
) -> SelectMenuLayout {
    let desired_count = visible_option_count(option_count);
    let desired_height = select_menu_height(desired_count);
    let space_below = (viewport.bottom - anchor.bottom - SELECT_MENU_GAP).max(0.0);
    let space_above = (anchor.top - viewport.top - SELECT_MENU_GAP).max(0.0);
    let resolved = match placement {
        SelectPlacement::Auto if space_below >= desired_height => SelectPlacement::Below,
        SelectPlacement::Auto if space_above >= desired_height => SelectPlacement::Above,
        SelectPlacement::Auto if space_below >= space_above => SelectPlacement::Below,
        SelectPlacement::Auto => SelectPlacement::Above,
        explicit => explicit,
    };
    let available_height = match resolved {
        SelectPlacement::Below => space_below,
        SelectPlacement::Above => space_above,
        SelectPlacement::Auto => unreachable!("auto placement must be resolved"),
    };
    let fitting_count = ((available_height - SELECT_MENU_PADDING * 2.0).max(0.0)
        / SELECT_OPTION_HEIGHT)
        .floor()
        .max(1.0) as usize;
    let visible_count = desired_count.min(fitting_count);
    let menu_height = select_menu_height(visible_count);
    let (top, bottom) = match resolved {
        SelectPlacement::Below => {
            let top = anchor.bottom + SELECT_MENU_GAP;
            (top, top + menu_height)
        }
        SelectPlacement::Above => {
            let bottom = anchor.top - SELECT_MENU_GAP;
            (bottom - menu_height, bottom)
        }
        SelectPlacement::Auto => unreachable!("auto placement must be resolved"),
    };

    SelectMenuLayout {
        rect: UiRect::new(anchor.left, top, anchor.right, bottom),
        visible_count,
        placement: resolved,
    }
}

pub(super) fn select_menu_height(visible_count: usize) -> f32 {
    if visible_count == 0 {
        0.0
    } else {
        SELECT_MENU_PADDING * 2.0 + SELECT_OPTION_HEIGHT * visible_count as f32
    }
}

fn visible_option_count(option_count: usize) -> usize {
    option_count.min(SELECT_MAX_VISIBLE_OPTIONS)
}

fn normalize_selected(selected: usize, option_count: usize) -> usize {
    if option_count == 0 {
        0
    } else {
        selected.min(option_count - 1)
    }
}

pub(super) fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

pub(super) fn mix_color(from: Color, to: Color, value: f32) -> Color {
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

pub(super) fn mix_u8(from: u8, to: u8, value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    (from as f32 + (to as f32 - from as f32) * value).round() as u8
}
