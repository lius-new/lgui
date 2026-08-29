use crate::core::{
    Color, Element, ElementKey, ElementRenderCx, RenderPhase, UiElement, UiId, UiRect, VisualStyle,
};

use super::DividerDirection;

pub struct FadedDivider {
    key: ElementKey,
    rect: UiRect,
    direction: DividerDirection,
    color: Color,
    max_alpha: u8,
    phase: RenderPhase,
}

impl FadedDivider {
    pub fn max_alpha(mut self, max_alpha: u8) -> Self {
        self.max_alpha = max_alpha;
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
        self
    }
}

impl From<FadedDivider> for Element {
    fn from(value: FadedDivider) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            render_faded_divider(cx.id, value)
        })
    }
}

#[track_caller]
pub fn faded_divider(rect: UiRect, direction: DividerDirection, color: Color) -> FadedDivider {
    FadedDivider {
        key: ElementKey::caller(),
        rect,
        direction,
        color,
        max_alpha: 56,
        phase: RenderPhase::Content,
    }
}

fn render_faded_divider(id: UiId, divider: FadedDivider) -> UiElement {
    let mut root = UiElement::group(id.clone(), divider.rect);
    match divider.direction {
        DividerDirection::Vertical => {
            let height = divider.rect.height().max(1.0).ceil() as i32;
            let x = (divider.rect.left + divider.rect.right) / 2.0;
            for offset in 0..height {
                let alpha = faded_alpha(offset, height, divider.max_alpha);
                if alpha == 0 {
                    continue;
                }
                root = root.child(
                    UiElement::panel(
                        UiId::owned(format!("{}.{}", id.as_str(), offset)),
                        UiRect::new(
                            x,
                            divider.rect.top + offset as f32,
                            x + 1.0,
                            divider.rect.top + offset as f32 + 1.0,
                        ),
                        VisualStyle::filled(divider.color).alpha(alpha),
                    )
                    .render_phase(divider.phase),
                );
            }
        }
        DividerDirection::Horizontal => {
            let width = divider.rect.width().max(1.0).ceil() as i32;
            let y = (divider.rect.top + divider.rect.bottom) / 2.0;
            for offset in 0..width {
                let alpha = faded_alpha(offset, width, divider.max_alpha);
                if alpha == 0 {
                    continue;
                }
                root = root.child(
                    UiElement::panel(
                        UiId::owned(format!("{}.{}", id.as_str(), offset)),
                        UiRect::new(
                            divider.rect.left + offset as f32,
                            y,
                            divider.rect.left + offset as f32 + 1.0,
                            y + 1.0,
                        ),
                        VisualStyle::filled(divider.color).alpha(alpha),
                    )
                    .render_phase(divider.phase),
                );
            }
        }
    }
    root
}

fn faded_alpha(offset: i32, length: i32, max_alpha: u8) -> u8 {
    let denominator = (length - 1).max(1) as f32;
    let distance = ((offset as f32 / denominator) - 0.5).abs() * 2.0;
    ((1.0 - distance) * max_alpha as f32)
        .round()
        .clamp(0.0, max_alpha as f32) as u8
}
