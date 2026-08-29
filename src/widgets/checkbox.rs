use std::sync::Arc;

use crate::{
    core::{
        Color, Element, ElementKey, ElementRenderCx, InteractionRole, PathStyle, Point,
        RenderPhase, SemanticAction, SemanticRole, Semantics, Stroke, UiElement, UiEventContext,
        UiId, UiPath, UiPathCommand, UiRect, VisualStyle,
    },
    theme::{ThemeContext, ThemeTokens},
};

pub type CheckboxChangeHandler = Arc<dyn Fn(&mut UiEventContext, bool) + Send + Sync>;

pub trait IntoCheckboxChangeHandler {
    fn into_checkbox_change_handler(self) -> CheckboxChangeHandler;
}

impl<F> IntoCheckboxChangeHandler for F
where
    F: Fn(bool) + Send + Sync + 'static,
{
    fn into_checkbox_change_handler(self) -> CheckboxChangeHandler {
        Arc::new(move |_context, checked| self(checked))
    }
}

impl IntoCheckboxChangeHandler for CheckboxChangeHandler {
    fn into_checkbox_change_handler(self) -> CheckboxChangeHandler {
        self
    }
}

#[derive(Clone, Copy)]
pub struct CheckboxStyle {
    pub fill: Color,
    pub border: Color,
    pub checked_fill: Color,
    pub checked_border: Color,
    pub check: Color,
    pub disabled_alpha: u8,
}

impl CheckboxStyle {
    pub const fn new(fill: Color, border: Color, check: Color) -> Self {
        Self {
            fill,
            border,
            checked_fill: fill,
            checked_border: border,
            check,
            disabled_alpha: 0x70,
        }
    }

    pub const fn checked(mut self, fill: Color, border: Color) -> Self {
        self.checked_fill = fill;
        self.checked_border = border;
        self
    }

    pub const fn disabled_alpha(mut self, disabled_alpha: u8) -> Self {
        self.disabled_alpha = disabled_alpha;
        self
    }

    pub fn from_theme(theme: &ThemeTokens) -> Self {
        Self::new(
            theme.colors.surface_interactive,
            theme.colors.border,
            theme.colors.accent_contrast,
        )
        .checked(theme.colors.accent, theme.colors.accent)
    }
}

impl Default for CheckboxStyle {
    fn default() -> Self {
        Self::from_theme(&ThemeTokens::default())
    }
}

pub struct Checkbox {
    key: ElementKey,
    rect: UiRect,
    hit_rect: UiRect,
    checked: bool,
    enabled: bool,
    style: Option<CheckboxStyle>,
    phase: RenderPhase,
    on_change: CheckboxChangeHandler,
}

impl Checkbox {
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

    pub fn style(mut self, style: CheckboxStyle) -> Self {
        self.style = Some(style);
        self
    }
}

impl From<Checkbox> for Element {
    fn from(value: Checkbox) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            render_checkbox(cx, value)
        })
    }
}

#[track_caller]
pub fn checkbox(
    rect: UiRect,
    checked: bool,
    on_change: impl IntoCheckboxChangeHandler,
) -> Checkbox {
    Checkbox {
        key: ElementKey::caller(),
        rect,
        hit_rect: rect,
        checked,
        enabled: true,
        style: None,
        phase: RenderPhase::Content,
        on_change: on_change.into_checkbox_change_handler(),
    }
}

fn render_checkbox(cx: ElementRenderCx<'_, '_, '_>, checkbox: Checkbox) -> UiElement {
    let style = checkbox.style.unwrap_or_else(|| {
        cx.try_use_context::<ThemeContext>()
            .map(|theme| CheckboxStyle::from_theme(theme.tokens()))
            .unwrap_or_default()
    });
    let fill = if checkbox.checked {
        style.checked_fill
    } else {
        style.fill
    };
    let border = if checkbox.checked {
        style.checked_border
    } else {
        style.border
    };
    let alpha = if checkbox.enabled {
        0xFF
    } else {
        style.disabled_alpha
    };
    let mut root = UiElement::panel(
        cx.id.clone(),
        checkbox.rect,
        VisualStyle::filled(fill)
            .alpha(alpha)
            .stroked(Stroke::new(border, 1.0, alpha))
            .radius(3.0),
    )
    .render_phase(checkbox.phase)
    .hit_rect(checkbox.hit_rect)
    .semantics({
        let mut semantics = Semantics::new(SemanticRole::CheckBox)
            .action(SemanticAction::Focus)
            .action(SemanticAction::Click);
        semantics.state.checked = Some(checkbox.checked);
        semantics.state.disabled = !checkbox.enabled;
        semantics
    });

    if checkbox.checked {
        root = root.child(checkmark(
            &cx.id,
            checkbox.rect,
            checkbox.phase,
            Stroke::new(style.check, 2.0, alpha),
        ));
    }
    if checkbox.enabled {
        let next_checked = next_checked(checkbox.checked);
        let on_change = checkbox.on_change;
        root = root
            .interaction(InteractionRole::Button)
            .on_click(move |context| on_change(context, next_checked));
    }
    root
}

fn checkmark(id: &UiId, rect: UiRect, phase: RenderPhase, stroke: Stroke) -> UiElement {
    let width = rect.width().max(1.0);
    let height = rect.height().max(1.0);
    let start = Point::new(rect.left + width * 2.0 / 9.0, rect.top + height / 2.0);
    let bend = Point::new(
        rect.left + width * 4.0 / 9.0,
        rect.top + height * 7.0 / 10.0,
    );
    let end = Point::new(
        rect.left + width * 7.0 / 9.0,
        rect.top + height * 3.0 / 10.0,
    );
    let path = UiPath::new([
        UiPathCommand::MoveTo(start),
        UiPathCommand::LineTo(bend),
        UiPathCommand::LineTo(end),
    ]);

    UiElement::path(
        UiId::owned(format!("{}.check", id.as_str())),
        rect,
        path,
        PathStyle::default().stroked(stroke),
    )
    .render_phase(phase)
}

fn next_checked(checked: bool) -> bool {
    !checked
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn checkmark_is_a_continuous_rising_tick_inside_the_checkbox_bounds() {
        let rect = UiRect::new(10.0, 20.0, 28.0, 38.0);
        let checkmark = checkmark(
            &UiId::owned("checkbox"),
            rect,
            RenderPhase::Content,
            Stroke::new(Color(0xFFFFFF), 2.0, 0xFF),
        );
        let commands = checkmark.node().path.as_ref().unwrap().commands();
        let [UiPathCommand::MoveTo(start), UiPathCommand::LineTo(bend), UiPathCommand::LineTo(end)] =
            commands
        else {
            panic!("checkbox checkmark must be one continuous three-point path");
        };

        for point in [start, bend, end] {
            assert!(rect.contains(*point));
        }
        assert!(start.x < bend.x && bend.x < end.x);
        assert!(start.y < bend.y);
        assert!(end.y < bend.y);
    }

    #[test]
    fn checkbox_reports_the_opposite_controlled_value() {
        assert!(!next_checked(true));
        assert!(next_checked(false));

        let reported = Arc::new(AtomicBool::new(false));
        let next = Arc::clone(&reported);
        let checkbox = checkbox(UiRect::new(0.0, 0.0, 16.0, 16.0), false, move |checked| {
            next.store(checked, Ordering::SeqCst);
        });
        let mut context = UiEventContext::new(
            crate::application::ApplicationContext::empty(),
            crate::application::WindowId::new("test"),
        );
        (checkbox.on_change)(&mut context, next_checked(checkbox.checked));
        assert!(reported.load(Ordering::SeqCst));
    }
}
