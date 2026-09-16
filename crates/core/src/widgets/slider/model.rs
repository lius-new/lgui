use std::{ops::RangeInclusive, sync::Arc};

use crate::core::{
    Color, Element, ElementKey, ElementRenderCx, RenderPhase, UiEventContext, UiRect,
};
use crate::theme::ThemeTokens;

use super::{
    math::{normalize_range, valid_step},
    render::render_slider,
};

pub type SliderChangeHandler = Arc<dyn Fn(&mut UiEventContext, f64) + Send + Sync>;
pub type SliderValueFormatter = Arc<dyn Fn(f64) -> String + Send + Sync>;
pub(super) const SLIDER_HOVER_IN_MS: f32 = 110.0;
pub(super) const SLIDER_HOVER_OUT_MS: f32 = 150.0;
pub(super) const SLIDER_VALUE_WIDTH: f32 = 48.0;
pub(super) const SLIDER_VALUE_GAP: f32 = 10.0;
pub(super) const CHANGE_EVENT: &str = "slider.change";
pub(super) const COMMIT_EVENT: &str = "slider.commit";

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
    pub track_height: f32,
    pub thumb_size: f32,
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
            track_height: 4.0,
            thumb_size: 14.0,
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
    pub(super) key: ElementKey,
    pub(super) rect: UiRect,
    pub(super) value: f64,
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) step: Option<f64>,
    pub(super) enabled: bool,
    pub(super) phase: RenderPhase,
    pub(super) style: Option<SliderStyle>,
    pub(super) value_formatter: Option<SliderValueFormatter>,
    pub(super) on_change: SliderChangeHandler,
    pub(super) on_commit: Option<SliderChangeHandler>,
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
