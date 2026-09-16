use std::{borrow::Cow, sync::Arc};

use crate::core::{
    Color, Element, ElementKey, ElementRenderCx, RenderPhase, UiEventContext, UiRect,
};
use crate::theme::ThemeTokens;

use super::render::render_select;

pub type SelectChangeHandler = Arc<dyn Fn(&mut UiEventContext, usize) + Send + Sync>;
pub const SELECT_OPTION_HEIGHT: f32 = 40.0;
pub(super) const SELECT_MAX_VISIBLE_OPTIONS: usize = 8;
pub(super) const SELECT_WHEEL_ROWS: i32 = 3;
pub(super) const SELECT_MENU_GAP: f32 = 4.0;
pub(super) const SELECT_MENU_PADDING: f32 = 4.0;
pub(super) const SELECT_HOVER_IN_MS: f32 = 110.0;
pub(super) const SELECT_HOVER_OUT_MS: f32 = 150.0;

pub(super) const TOGGLE_ACTION: &str = "select.toggle";
pub(super) const SELECT_ACTION: &str = "select.choose";
pub(super) const SCROLL_ACTION: &str = "select.scroll";
pub(super) const CHANGE_EVENT: &str = "select.change";

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
    pub radius: f32,
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
            radius: 6.0,
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
    pub(super) key: ElementKey,
    pub(super) rect: UiRect,
    pub(super) selected: usize,
    pub(super) options: Vec<SelectOption>,
    pub(super) enabled: bool,
    pub(super) phase: RenderPhase,
    pub(super) placement: SelectPlacement,
    pub(super) style: Option<SelectStyle>,
    pub(super) on_change: SelectChangeHandler,
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
