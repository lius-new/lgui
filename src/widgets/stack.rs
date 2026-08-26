use crate::core::{
    Align, Axis, EdgeInsets, Element, ElementKey, ElementRenderCx, IntoElementContent, LayoutSpec,
    UiElement, UiRect,
};

pub struct Stack {
    key: ElementKey,
    rect: UiRect,
    axis: Axis,
    gap: i32,
    padding: EdgeInsets,
    align: Align,
    children: Vec<Element>,
}

impl Stack {
    pub fn gap(mut self, gap: i32) -> Self {
        self.gap = gap;
        self
    }

    pub fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn content(mut self, content: impl IntoElementContent) -> Self {
        content.append_to(&mut self.children);
        self
    }
}

impl From<Stack> for Element {
    fn from(value: Stack) -> Self {
        let Stack {
            key,
            rect,
            axis,
            gap,
            padding,
            align,
            children,
        } = value;
        Element::with_key(key, move |cx: ElementRenderCx<'_, '_, '_>| {
            UiElement::group(cx.id, rect)
                .layout(LayoutSpec::Stack {
                    axis,
                    gap,
                    padding,
                    align,
                })
                .children(cx.children)
        })
        .children(children)
    }
}

impl IntoElementContent for Stack {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}

#[track_caller]
pub fn stack(rect: UiRect, axis: Axis) -> Stack {
    Stack {
        key: ElementKey::caller(),
        rect,
        axis,
        gap: 0,
        padding: EdgeInsets::ZERO,
        align: Align::Start,
        children: Vec::new(),
    }
}
