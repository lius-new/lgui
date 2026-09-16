use std::future::Future;

use crate::core::{async_handler, UiAsyncContext, UiEventContext};
use crate::core::{
    AnimProperty, AnimationBinding, Element, ElementKey, ElementRenderCx, InteractionRole,
    IntoElementContent, RenderPhase, UiAction, UiElement, UiEventHandler, UiId, UiRect,
    VisualStyle,
};

pub struct Panel {
    key: ElementKey,
    rect: UiRect,
    style: VisualStyle,
    phase: RenderPhase,
    interaction: Option<InteractionRole>,
    click_action: Option<UiAction>,
    action_target: Option<UiId>,
    click_handler: Option<UiEventHandler>,
    animations: Vec<AnimationBinding>,
    animation_targets: Vec<(AnimProperty, bool)>,
    animation_outset: Option<(f32, f32)>,
    paint_bounds: Option<UiRect>,
    children: Vec<Element>,
}

impl Panel {
    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
        self
    }

    pub fn style(mut self, style: VisualStyle) -> Self {
        self.style = style;
        self
    }

    pub fn map_style(mut self, map: impl FnOnce(VisualStyle) -> VisualStyle) -> Self {
        self.style = map(self.style);
        self
    }

    pub fn interaction(mut self, interaction: InteractionRole) -> Self {
        self.interaction = Some(interaction);
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.click_handler = Some(std::sync::Arc::new(handler));
        self
    }

    pub fn on_click_async<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(UiAsyncContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.click_handler = Some(async_handler(handler));
        self
    }

    pub fn on_click_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_handler = Some(handler);
        self
    }

    pub fn click_action(mut self, action: UiAction) -> Self {
        self.click_action = Some(action);
        self
    }

    pub fn action_target(mut self, target: UiId) -> Self {
        self.action_target = Some(target);
        self
    }

    pub fn animation(mut self, binding: AnimationBinding) -> Self {
        self.animations.push(binding);
        self
    }

    pub fn animation_target(mut self, property: AnimProperty, active: bool) -> Self {
        self.animation_targets.push((property, active));
        self
    }

    pub fn animation_outset(mut self, x: f32, y: f32) -> Self {
        self.animation_outset = Some((x, y));
        self
    }

    pub fn paint_bounds(mut self, rect: UiRect) -> Self {
        self.paint_bounds = Some(rect);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Element>>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }
}

impl From<Panel> for Element {
    fn from(value: Panel) -> Self {
        let Panel {
            key,
            rect,
            style,
            phase,
            interaction,
            click_action,
            action_target,
            click_handler,
            animations,
            animation_targets,
            animation_outset,
            paint_bounds,
            children,
        } = value;
        Element::with_key(key, move |cx: ElementRenderCx<'_, '_, '_>| {
            let mut element = UiElement::panel(cx.id, rect, style).render_phase(phase);
            if let Some(interaction) = interaction {
                element = element.interaction(interaction);
            }
            if let Some(handler) = click_handler {
                element = element.on_click_handler(handler);
            }
            if let Some(action) = click_action {
                element = element.click_action(action);
            }
            if let Some(target) = action_target {
                element = element.action_target(target);
            }
            for animation in animations {
                element = element.animation(animation);
            }
            for (property, active) in animation_targets {
                element = element.animation_target(property, active);
            }
            if let Some((x, y)) = animation_outset {
                element = element.animation_outset(x, y);
            }
            if let Some(rect) = paint_bounds {
                element = element.paint_bounds(rect);
            }
            element.children(cx.children)
        })
        .children(children)
    }
}

impl IntoElementContent for Panel {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}

#[track_caller]
pub fn panel(rect: UiRect, style: VisualStyle) -> Element {
    Element::from(Panel {
        key: ElementKey::caller(),
        rect,
        style,
        phase: RenderPhase::Content,
        interaction: None,
        click_action: None,
        action_target: None,
        click_handler: None,
        animations: Vec::new(),
        animation_targets: Vec::new(),
        animation_outset: None,
        paint_bounds: None,
        children: Vec::new(),
    })
}
