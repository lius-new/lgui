use crate::core::{
    Align, Axis, EdgeInsets, Element, ElementKey, ElementRenderCx, FlexLayout, IntoElementContent,
    Justify, LayoutSpec, UiElement, UiRect,
};

pub struct Flex {
    key: ElementKey,
    rect: UiRect,
    axis: Axis,
    gap: f32,
    padding: EdgeInsets,
    align: Align,
    justify: Justify,
    wrap: bool,
    children: Vec<Element>,
}

impl Flex {
    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
    pub fn gap(mut self, gap: f32) -> Self {
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

impl From<Flex> for Element {
    fn from(value: Flex) -> Self {
        let Flex {
            key,
            rect,
            axis,
            gap,
            padding,
            align,
            justify,
            wrap,
            children,
        } = value;
        Element::with_key(key, move |cx: ElementRenderCx<'_, '_, '_>| {
            UiElement::group(cx.id, rect)
                .layout(LayoutSpec::Flex(FlexLayout {
                    axis,
                    gap,
                    padding,
                    align,
                    justify,
                    wrap,
                }))
                .children(cx.children)
        })
        .children(children)
    }
}

impl IntoElementContent for Flex {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}

#[track_caller]
pub fn flex(rect: UiRect, axis: Axis) -> Flex {
    Flex {
        key: ElementKey::caller(),
        rect,
        axis,
        gap: 0.0,
        padding: EdgeInsets::ZERO,
        align: Align::Start,
        justify: Justify::Start,
        wrap: false,
        children: Vec::new(),
    }
}
