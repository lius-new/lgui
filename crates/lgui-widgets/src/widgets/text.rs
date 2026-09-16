use std::borrow::Cow;

use crate::core::{
    Element, ElementKey, ElementRenderCx, IntoElementContent, RenderPhase, TextStyle, UiElement,
    UiRect,
};

pub struct Text {
    key: ElementKey,
    rect: UiRect,
    value: Cow<'static, str>,
    style: TextStyle,
    phase: RenderPhase,
}

impl Text {
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn map_style(mut self, map: impl FnOnce(TextStyle) -> TextStyle) -> Self {
        self.style = map(self.style);
        self
    }

    pub fn value(mut self, value: impl Into<Cow<'static, str>>) -> Self {
        self.value = value.into();
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
        self
    }
}

impl From<Text> for Element {
    fn from(value: Text) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            UiElement::text(cx.id, value.rect, value.value, value.style).render_phase(value.phase)
        })
    }
}

impl IntoElementContent for Text {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}

#[track_caller]
pub fn text(rect: UiRect, value: impl Into<Cow<'static, str>>, style: TextStyle) -> Text {
    Text {
        key: ElementKey::caller(),
        rect,
        value: value.into(),
        style,
        phase: RenderPhase::Content,
    }
}
