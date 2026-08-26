use std::{any::Any, borrow::Cow, sync::Arc};

use crate::core::{
    AnimProperty, AnimationBinding, Color, ComponentActionOutcome, ComponentState, Element,
    ElementKey, ElementRenderCx, EventPolicy, IconStyle, InteractionRole, RenderPhase, Stroke,
    TextStyle, UiAction, UiElement, UiEventContext, UiId, UiRect, UiRenderContext, VisualStyle,
};
use crate::theme::{ThemeContext, ThemeTokens};

pub type SelectChangeHandler = Arc<dyn Fn(&mut UiEventContext, usize) + Send + Sync>;
pub const SELECT_OPTION_HEIGHT: i32 = 40;
const SELECT_MAX_VISIBLE_OPTIONS: usize = 8;
const SELECT_WHEEL_ROWS: i32 = 3;
const SELECT_MENU_GAP: i32 = 4;
const SELECT_MENU_PADDING: i32 = 4;
const SELECT_HOVER_IN_MS: f32 = 110.0;
const SELECT_HOVER_OUT_MS: f32 = 150.0;

const TOGGLE_ACTION: &str = "select.toggle";
const SELECT_ACTION: &str = "select.choose";
const SCROLL_ACTION: &str = "select.scroll";
const CHANGE_EVENT: &str = "select.change";

pub trait IntoSelectChangeHandler {
    fn into_select_change_handler(self) -> SelectChangeHandler;
}

impl<F> IntoSelectChangeHandler for F
where
    F: Fn(usize) + Send + Sync + 'static,
{
    fn into_select_change_handler(self) -> SelectChangeHandler {
        Arc::new(move |_context, index| self(index))
    }
}

impl IntoSelectChangeHandler for SelectChangeHandler {
    fn into_select_change_handler(self) -> SelectChangeHandler {
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectSwatch {
    pub primary: Color,
    pub secondary: Color,
}

impl SelectSwatch {
    pub const fn new(primary: Color, secondary: Color) -> Self {
        Self { primary, secondary }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub label: Cow<'static, str>,
    pub swatch: Option<SelectSwatch>,
}

impl SelectOption {
    pub fn new(label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            label: label.into(),
            swatch: None,
        }
    }

    pub const fn swatch(mut self, swatch: SelectSwatch) -> Self {
        self.swatch = Some(swatch);
        self
    }
}

impl From<&'static str> for SelectOption {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SelectOption {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Copy)]
pub struct SelectStyle {
    pub control_fill: Color,
    pub control_hover_fill: Color,
    pub menu_fill: Color,
    pub option_hover_fill: Color,
    pub selected_fill: Color,
    pub border: Color,
    pub open_border: Color,
    pub text: Color,
    pub selected_text: Color,
    pub icon: Color,
    pub active_icon: Color,
    pub swatch_border: Color,
    pub radius: i32,
}

impl SelectStyle {
    pub fn from_theme(theme: &ThemeTokens) -> Self {
        Self {
            control_fill: theme.colors.surface_interactive,
            control_hover_fill: theme.colors.surface_hover,
            menu_fill: theme.colors.surface_raised,
            option_hover_fill: theme.colors.surface_hover,
            selected_fill: theme.colors.surface_hover,
            border: theme.colors.border_subtle,
            open_border: theme.colors.accent,
            text: theme.colors.text_secondary,
            selected_text: theme.colors.text,
            icon: theme.colors.text_muted,
            active_icon: theme.colors.accent,
            swatch_border: theme.colors.border_subtle,
            radius: 6,
        }
    }
}

impl Default for SelectStyle {
    fn default() -> Self {
        Self::from_theme(&ThemeTokens::default())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectPlacement {
    #[default]
    Auto,
    Below,
    Above,
}

pub struct Select {
    key: ElementKey,
    rect: UiRect,
    selected: usize,
    options: Vec<SelectOption>,
    enabled: bool,
    phase: RenderPhase,
    placement: SelectPlacement,
    style: Option<SelectStyle>,
    on_change: SelectChangeHandler,
}

impl Select {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
        self
    }

    pub fn placement(mut self, placement: SelectPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn style(mut self, style: SelectStyle) -> Self {
        self.style = Some(style);
        self
    }
}

impl From<Select> for Element {
    fn from(value: Select) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            render_select(cx, value)
        })
    }
}

#[track_caller]
pub fn select<I, O>(
    rect: UiRect,
    selected: usize,
    options: I,
    on_change: impl IntoSelectChangeHandler,
) -> Select
where
    I: IntoIterator<Item = O>,
    O: Into<SelectOption>,
{
    Select {
        key: ElementKey::caller(),
        rect,
        selected,
        options: options.into_iter().map(Into::into).collect(),
        enabled: true,
        phase: RenderPhase::Content,
        placement: SelectPlacement::Auto,
        style: None,
        on_change: on_change.into_select_change_handler(),
    }
}

fn render_select(cx: ElementRenderCx<'_, '_, '_>, select: Select) -> UiElement {
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
        select.rect.left + 40
    } else {
        select.rect.left + 12
    };

    let mut root = UiElement::panel(
        id.clone(),
        select.rect,
        VisualStyle::filled(control_fill)
            .alpha(0xF2)
            .stroked(Stroke::new(control_border, 1, border_alpha))
            .radius(style.radius),
    )
    .render_phase(select.phase)
    .paint_bounds(paint_bounds)
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
                    select.rect.left + 12,
                    select.rect.top + 10,
                    select.rect.left + 30,
                    select.rect.bottom - 10,
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
                    select.rect.right - 34,
                    select.rect.bottom,
                ),
                option.label.clone(),
                TextStyle::new(style.selected_text, -14, 500),
            )
            .render_phase(select.phase),
        );
    }

    root = root.child(
        UiElement::icon(
            UiId::owned(format!("{}.chevron", id.as_str())),
            UiRect::new(
                select.rect.right - 28,
                select.rect.top + (select.rect.height() - 16) / 2,
                select.rect.right - 12,
                select.rect.top + (select.rect.height() + 16) / 2,
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
            .stroked(Stroke::new(style.border, 1, 0x70))
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
        let top = rect.top + SELECT_MENU_PADDING + slot as i32 * SELECT_OPTION_HEIGHT;
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
            item.left + 38
        } else {
            item.left + 10
        };
        let text_right = if is_selected {
            item.right - 36
        } else {
            item.right - 10
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
                .radius((style.radius - 2).max(2)),
        )
        .render_phase(phase)
        .interaction(InteractionRole::Button)
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
                            item.left + 2,
                            item.top + 10,
                            item.left + 4,
                            item.bottom - 10,
                        ),
                        VisualStyle::filled(style.active_icon).radius(1),
                    )
                    .render_phase(phase),
                )
                .child(
                    UiElement::icon(
                        UiId::owned(format!("{}.option.{index}.check", select_id.as_str())),
                        UiRect::new(
                            item.right - 27,
                            item.top + 11,
                            item.right - 11,
                            item.bottom - 11,
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
                    item.left + 10,
                    item.top + 11,
                    item.left + 28,
                    item.bottom - 11,
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
                    -14,
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
        rect.right - 6,
        rect.top + 8,
        rect.right - 3,
        rect.bottom - 8,
    );
    let thumb_height = (track.height() * visible_count as i32 / option_count as i32)
        .max(24)
        .min(track.height());
    let max_start = option_count.saturating_sub(visible_count);
    let thumb_travel = track.height() - thumb_height;
    let thumb_top = if max_start == 0 {
        track.top
    } else {
        track.top + thumb_travel * scroll_start as i32 / max_start as i32
    };

    UiElement::panel(
        UiId::owned(format!("{}.scrollbar.track", select_id.as_str())),
        track,
        VisualStyle::filled(style.border).alpha(0x38).radius(2),
    )
    .render_phase(phase)
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.scrollbar.thumb", select_id.as_str())),
            UiRect::new(track.left, thumb_top, track.right, thumb_top + thumb_height),
            VisualStyle::filled(style.icon).alpha(0x92).radius(2),
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
    let center = (rect.left + rect.right) / 2;
    UiElement::panel(
        id.clone(),
        rect,
        VisualStyle::default()
            .stroked(Stroke::new(border, 1, 0xFF))
            .radius(2),
    )
    .render_phase(phase)
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.primary", id.as_str())),
            UiRect::new(rect.left + 1, rect.top + 1, center, rect.bottom - 1),
            VisualStyle::filled(swatch.primary),
        )
        .render_phase(phase),
    )
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.secondary", id.as_str())),
            UiRect::new(center, rect.top + 1, rect.right - 1, rect.bottom - 1),
            VisualStyle::filled(swatch.secondary),
        )
        .render_phase(phase),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectMenuLayout {
    rect: UiRect,
    visible_count: usize,
    placement: SelectPlacement,
}

fn select_menu_layout(
    anchor: UiRect,
    option_count: usize,
    viewport: UiRect,
    placement: SelectPlacement,
) -> SelectMenuLayout {
    let desired_count = visible_option_count(option_count);
    let desired_height = select_menu_height(desired_count);
    let space_below = (viewport.bottom - anchor.bottom - SELECT_MENU_GAP).max(0);
    let space_above = (anchor.top - viewport.top - SELECT_MENU_GAP).max(0);
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
    let fitting_count = ((available_height - SELECT_MENU_PADDING * 2).max(0) / SELECT_OPTION_HEIGHT)
        .max(1) as usize;
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

fn select_menu_height(visible_count: usize) -> i32 {
    if visible_count == 0 {
        0
    } else {
        SELECT_MENU_PADDING * 2 + SELECT_OPTION_HEIGHT * visible_count as i32
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

#[derive(Clone)]
struct SelectState {
    open: bool,
    selected: usize,
    option_count: usize,
    visible_count: usize,
    scroll_start: usize,
    enabled: bool,
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
    fn configure(
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let viewport = UiRect::new(0, 0, 800, 700);
        let anchor = UiRect::new(500, 600, 720, 638);
        let menu = select_menu_layout(anchor, 100, viewport, SelectPlacement::Auto);

        assert_eq!(menu.placement, SelectPlacement::Above);
        assert_eq!(menu.visible_count, 8);
        assert_eq!(menu.rect.bottom, anchor.top - SELECT_MENU_GAP);
        assert_eq!(menu.rect.height(), select_menu_height(8));
    }

    #[test]
    fn auto_placement_prefers_below_when_the_full_menu_fits() {
        let viewport = UiRect::new(0, 0, 800, 700);
        let anchor = UiRect::new(500, 100, 720, 138);
        let menu = select_menu_layout(anchor, 100, viewport, SelectPlacement::Auto);

        assert_eq!(menu.placement, SelectPlacement::Below);
        assert_eq!(menu.visible_count, 8);
        assert_eq!(menu.rect.top, anchor.bottom + SELECT_MENU_GAP);
    }

    #[test]
    fn explicit_placement_is_respected_and_reduces_visible_rows_to_fit() {
        let viewport = UiRect::new(0, 0, 800, 700);
        let anchor = UiRect::new(500, 600, 720, 638);
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
}
