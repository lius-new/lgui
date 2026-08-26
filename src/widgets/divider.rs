use crate::core::{
    Element, ElementKey, ElementRenderCx, Stroke, UiElement, UiId, UiRect, VisualStyle,
};

#[derive(Clone, Copy)]
pub enum DividerDirection {
    Horizontal,
    Vertical,
}

pub struct Divider {
    key: ElementKey,
    rect: UiRect,
    direction: DividerDirection,
    stroke: Stroke,
}

impl From<Divider> for Element {
    fn from(value: Divider) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            render_divider(cx.id, value)
        })
    }
}

#[track_caller]
pub fn divider(rect: UiRect, direction: DividerDirection, stroke: Stroke) -> Divider {
    Divider {
        key: ElementKey::caller(),
        rect,
        direction,
        stroke,
    }
}

fn render_divider(id: UiId, divider: Divider) -> UiElement {
    let rect = match divider.direction {
        DividerDirection::Horizontal => UiRect::new(
            divider.rect.left,
            (divider.rect.top + divider.rect.bottom) / 2,
            divider.rect.right,
            (divider.rect.top + divider.rect.bottom) / 2,
        ),
        DividerDirection::Vertical => UiRect::new(
            (divider.rect.left + divider.rect.right) / 2,
            divider.rect.top,
            (divider.rect.left + divider.rect.right) / 2,
            divider.rect.bottom,
        ),
    };
    UiElement::line(id, rect, VisualStyle::default().stroked(divider.stroke))
}
