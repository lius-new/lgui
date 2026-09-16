use std::future::Future;

use crate::core::{async_handler, UiAsyncContext, UiEventContext};
use crate::core::{
    AnimProperty, AnimationBinding, Element, ElementKey, ElementRenderCx, InteractionRole,
    IntoElementContent, SemanticAction, SemanticRole, Semantics, TextStyle, UiElement,
    UiEventHandler, UiRect, VisualStyle,
};

#[derive(Clone, Copy)]
pub struct ButtonStyle {
    pub panel: VisualStyle,
    pub text: TextStyle,
    pub hover_outset: (f32, f32),
}

impl ButtonStyle {
    pub fn map_panel(mut self, map: impl FnOnce(VisualStyle) -> VisualStyle) -> Self {
        self.panel = map(self.panel);
        self
    }

    pub fn map_text(mut self, map: impl FnOnce(TextStyle) -> TextStyle) -> Self {
        self.text = map(self.text);
        self
    }

    pub fn hover_outset(mut self, x: f32, y: f32) -> Self {
        self.hover_outset = (x, y);
        self
    }
}

pub struct Button {
    key: ElementKey,
    rect: UiRect,
    label: &'static str,
    click_handler: Option<UiEventHandler>,
    style: ButtonStyle,
    children: Vec<Element>,
}

impl Button {
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

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn map_style(mut self, map: impl FnOnce(ButtonStyle) -> ButtonStyle) -> Self {
        self.style = map(self.style);
        self
    }

    pub fn label(mut self, label: &'static str) -> Self {
        self.label = label;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<Button> for Element {
    fn from(value: Button) -> Self {
        let Button {
            key,
            rect,
            label,
            click_handler,
            style,
            children,
        } = value;
        Element::with_key(key, move |cx: ElementRenderCx<'_, '_, '_>| {
            let label_id = cx.scope.id(format!("{}.label", cx.id.as_str()));
            let mut root = UiElement::button(cx.id, rect, style.panel)
                .interaction(InteractionRole::Button)
                .semantics(
                    Semantics::new(SemanticRole::Button)
                        .name(label)
                        .action(SemanticAction::Click),
                )
                .animation(AnimationBinding::new(AnimProperty::Hover, 0.0, 1.0))
                .animation(AnimationBinding::new(AnimProperty::Pressed, 0.0, 1.0))
                .animation_outset(style.hover_outset.0, style.hover_outset.1);
            if !label.is_empty() {
                root = root.child(UiElement::text(label_id, rect, label, style.text));
            }
            root = root.children(cx.children);
            if let Some(handler) = click_handler {
                root = root.on_click_handler(handler);
            }
            root
        })
        .children(children)
    }
}

impl IntoElementContent for Button {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}

#[track_caller]
pub fn button(rect: UiRect, label: &'static str, style: ButtonStyle) -> Button {
    Button {
        key: ElementKey::caller(),
        rect,
        label,
        click_handler: None,
        style,
        children: Vec::new(),
    }
}
