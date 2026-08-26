use std::sync::Arc;

use crate::{
    core::{
        AnimProperty, AnimationBinding, Color, Element, ElementKey, ElementRenderCx,
        InteractionRole, RenderPhase, Stroke, UiElement, UiEventContext, UiId, UiRect, VisualStyle,
    },
    theme::{ThemeContext, ThemeTokens},
};

pub type SwitchChangeHandler = Arc<dyn Fn(&mut UiEventContext, bool) + Send + Sync>;
pub const SWITCH_WIDTH: i32 = 46;
pub const SWITCH_HEIGHT: i32 = 24;
const SWITCH_ANIMATION_DURATION_MS: f32 = 160.0;

pub trait IntoSwitchChangeHandler {
    fn into_switch_change_handler(self) -> SwitchChangeHandler;
}

impl<F> IntoSwitchChangeHandler for F
where
    F: Fn(bool) + Send + Sync + 'static,
{
    fn into_switch_change_handler(self) -> SwitchChangeHandler {
        Arc::new(move |_context, checked| self(checked))
    }
}

impl IntoSwitchChangeHandler for SwitchChangeHandler {
    fn into_switch_change_handler(self) -> SwitchChangeHandler {
        self
    }
}

#[derive(Clone, Copy)]
pub struct SwitchStyle {
    pub track: Color,
    pub checked_track: Color,
    pub border: Color,
    pub checked_border: Color,
    pub thumb: Color,
    pub checked_thumb: Color,
    pub disabled_alpha: u8,
}

impl SwitchStyle {
    pub const fn new(track: Color, checked_track: Color, thumb: Color) -> Self {
        Self {
            track,
            checked_track,
            border: track,
            checked_border: checked_track,
            thumb,
            checked_thumb: thumb,
            disabled_alpha: 0x70,
        }
    }

    pub const fn borders(mut self, border: Color, checked_border: Color) -> Self {
        self.border = border;
        self.checked_border = checked_border;
        self
    }

    pub const fn checked_thumb(mut self, checked_thumb: Color) -> Self {
        self.checked_thumb = checked_thumb;
        self
    }

    pub const fn disabled_alpha(mut self, disabled_alpha: u8) -> Self {
        self.disabled_alpha = disabled_alpha;
        self
    }

    pub fn from_theme(theme: &ThemeTokens) -> Self {
        Self::new(
            theme.colors.surface_raised,
            theme.colors.accent,
            theme.colors.text_secondary,
        )
        .borders(theme.colors.border, theme.colors.accent)
        .checked_thumb(theme.colors.accent_contrast)
        .disabled_alpha(0x60)
    }
}

impl Default for SwitchStyle {
    fn default() -> Self {
        Self::from_theme(&ThemeTokens::default())
    }
}

pub struct Switch {
    key: ElementKey,
    rect: UiRect,
    hit_rect: UiRect,
    checked: bool,
    enabled: bool,
    style: Option<SwitchStyle>,
    phase: RenderPhase,
    on_change: SwitchChangeHandler,
}

impl Switch {
    pub fn hit_rect(mut self, hit_rect: UiRect) -> Self {
        self.hit_rect = hit_rect;
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

    pub fn style(mut self, style: SwitchStyle) -> Self {
        self.style = Some(style);
        self
    }
}

impl From<Switch> for Element {
    fn from(value: Switch) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            let progress = smootherstep(cx.animation_value(AnimProperty::Active));
            render_switch(cx, value, progress)
        })
    }
}

#[track_caller]
pub fn switch(rect: UiRect, checked: bool, on_change: impl IntoSwitchChangeHandler) -> Switch {
    Switch {
        key: ElementKey::caller(),
        rect,
        hit_rect: rect,
        checked,
        enabled: true,
        style: None,
        phase: RenderPhase::Content,
        on_change: on_change.into_switch_change_handler(),
    }
}

fn render_switch(cx: ElementRenderCx<'_, '_, '_>, switch: Switch, progress: f32) -> UiElement {
    let style = switch.style.unwrap_or_else(|| {
        cx.try_use_context::<ThemeContext>()
            .map(|theme| SwitchStyle::from_theme(theme.tokens()))
            .unwrap_or_default()
    });
    let id = cx.id;
    let track = mix_color(style.track, style.checked_track, progress);
    let border = mix_color(style.border, style.checked_border, progress);
    let thumb = mix_color(style.thumb, style.checked_thumb, progress);
    let alpha = if switch.enabled {
        0xFF
    } else {
        style.disabled_alpha
    };
    let height = switch.rect.height().max(1);
    let inset = (height / 6).clamp(2, 4);
    let thumb_size = (height - inset * 2).max(1).min(switch.rect.width().max(1));
    let thumb_left = mix_i32(
        switch.rect.left + inset,
        switch.rect.right - inset - thumb_size,
        progress,
    );
    let thumb_top = switch.rect.top + (height - thumb_size) / 2;
    let thumb_rect = UiRect::new(
        thumb_left,
        thumb_top,
        thumb_left + thumb_size,
        thumb_top + thumb_size,
    );

    let mut root = UiElement::panel(
        id.clone(),
        switch.rect,
        VisualStyle::filled(track)
            .alpha(alpha)
            .stroked(Stroke::new(border, 1, alpha))
            .radius(height / 2),
    )
    .render_phase(switch.phase)
    .hit_rect(switch.hit_rect)
    .animation(
        AnimationBinding::new(AnimProperty::Active, 0.0, 1.0)
            .duration(SWITCH_ANIMATION_DURATION_MS, SWITCH_ANIMATION_DURATION_MS),
    )
    .animation_target(AnimProperty::Active, switch.checked)
    .child(
        UiElement::panel(
            UiId::owned(format!("{}.thumb", id.as_str())),
            thumb_rect,
            VisualStyle::filled(thumb)
                .alpha(alpha)
                .radius(thumb_size / 2),
        )
        .render_phase(switch.phase),
    );

    if switch.enabled {
        let next_checked = !switch.checked;
        let on_change = switch.on_change;
        root = root
            .interaction(InteractionRole::Button)
            .on_click(move |ctx| on_change(ctx, next_checked));
    }
    root
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

fn mix_i32(from: i32, to: i32, value: f32) -> i32 {
    let value = value.clamp(0.0, 1.0);
    (from as f32 + (to - from) as f32 * value).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        context_provider, HostTreeBuilder, RenderCx, RootComponent, UiRuntime, UiScale,
    };

    struct ThemedSwitchRoot {
        theme: ThemeContext,
    }

    impl RootComponent for ThemedSwitchRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> Element {
            context_provider(
                self.theme,
                Element::from(switch(UiRect::new(0, 0, 46, 24), false, |_| {})),
            )
        }
    }

    #[test]
    fn switch_visuals_interpolate_between_stable_endpoints() {
        assert_eq!(mix_i32(4, 26, 0.0), 4);
        assert_eq!(mix_i32(4, 26, 1.0), 26);
        assert_eq!(
            mix_color(Color(0x000000), Color(0xFFFFFF), 0.5),
            Color(0x808080)
        );
        assert_eq!(smootherstep(0.0), 0.0);
        assert_eq!(smootherstep(1.0), 1.0);
    }

    #[test]
    fn default_style_resolves_from_the_nearest_theme_context() {
        let mut tokens = ThemeTokens::default();
        tokens.colors.surface_raised = Color(0x123456);
        let ui = UiRuntime::new();
        let viewport = UiRect::new(0, 0, 46, 24);
        let interaction = ui.interaction_state();
        let mut builder = HostTreeBuilder::new();
        builder.mount(
            ThemedSwitchRoot {
                theme: ThemeContext::new(tokens),
            },
            viewport,
            &interaction,
            ui.animations(),
            ui.component_states(),
            ui.component_tree(),
            ui.contexts(),
            ui.hook_states(),
            ui.hook_updates(),
            ui.task_spawner(),
            ui.effects(),
            UiScale::ONE,
        );

        let tree = builder.finish();
        let track = tree
            .nodes()
            .iter()
            .find(|node| {
                node.kind == crate::core::UiNodeKind::Panel && node.layout_rect == viewport
            })
            .expect("switch track");
        assert_eq!(track.style.fill, Some(Color(0x123456)));
    }
}
