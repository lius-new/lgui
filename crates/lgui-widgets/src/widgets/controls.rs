use crate::WidgetValue;
use crate::core::*;
use crate::theme::ThemeContext;
use std::{any::Any, sync::Arc};
const CHANGED: &str = "control.changed";
fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}
#[derive(Clone)]
struct DragState {
    axis: Axis,
    value: f32,
    min: f32,
    max: f32,
    anchor: Option<(f32, f32)>,
    scroll: bool,
    track: f32,
    thumb: f32,
}
impl Default for DragState {
    fn default() -> Self {
        Self {
            axis: Axis::Horizontal,
            value: 0.0,
            min: 0.0,
            max: 0.0,
            anchor: None,
            scroll: false,
            track: 0.0,
            thumb: 0.0,
        }
    }
}
impl ComponentState for DragState {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn handle_action(&mut self, a: &UiAction) -> ComponentActionOutcome {
        let coordinate = || {
            let (x, y) = a.payload_value()?.split_once(',')?;
            Some(finite(if self.axis == Axis::Horizontal {
                x.parse().ok()?
            } else {
                y.parse().ok()?
            }))
        };
        let before = self.value;
        match a.id().as_str() {
            POINTER_DOWN_ACTION => {
                let Some(p) = coordinate() else {
                    return ComponentActionOutcome::ignored();
                };
                if self.scroll {
                    let travel = (self.track - self.thumb).max(0.0);
                    let start = if self.max > 0.0 {
                        travel * self.value / self.max
                    } else {
                        0.0
                    };
                    if p < start || p > start + self.thumb {
                        self.value = if travel > 0.0 {
                            ((p - self.thumb / 2.0) / travel * self.max).clamp(self.min, self.max)
                        } else {
                            0.0
                        };
                    }
                }
                self.anchor = Some((p, self.value));
            }
            POINTER_DRAG_ACTION => {
                if let (Some(p), Some((start, value))) = (coordinate(), self.anchor) {
                    let scale = if self.scroll {
                        let travel = self.track - self.thumb;
                        if travel > 0.0 { self.max / travel } else { 0.0 }
                    } else {
                        1.0
                    };
                    self.value = (value + (p - start) * scale).clamp(self.min, self.max);
                }
            }
            POINTER_UP_ACTION => self.anchor = None,
            "text.set" | "semantic.set_value" => {
                if let Some(v) = a.payload_value().and_then(|s| s.parse::<f32>().ok()) {
                    self.value = finite(v).clamp(self.min, self.max);
                }
            }
            "semantic.increment" => self.value = (self.value + 1.0).min(self.max),
            "semantic.decrement" => self.value = (self.value - 1.0).max(self.min),
            _ => return ComponentActionOutcome::ignored(),
        }
        let changed = before != self.value;
        let out = ComponentActionOutcome::handled(changed);
        if changed {
            out.emit(UiAction::new(CHANGED).payload(self.value.to_string()))
        } else {
            out
        }
    }
}
pub fn resize_handle(
    rect: UiRect,
    axis: Axis,
    value: f32,
    min: f32,
    max: f32,
    on_change: impl Fn(f32) + Send + Sync + 'static,
) -> Element {
    Element::new(move |cx| {
        let min = finite(min);
        let max = finite(max).max(min);
        cx.context.component_state_mut(&cx.id, |s: &mut DragState| {
            s.axis = axis;
            s.value = finite(value).clamp(min, max);
            s.min = min;
            s.max = max;
            s.scroll = false;
        });
        let mut semantics = Semantics::new(SemanticRole::Slider)
            .action(SemanticAction::Increment)
            .action(SemanticAction::Decrement)
            .action(SemanticAction::SetValue);
        semantics.numeric_value = Some(value as f64);
        semantics.numeric_min = Some(min as f64);
        semantics.numeric_max = Some(max as f64);
        UiElement::group(cx.id.clone(), rect)
            .interaction(InteractionRole::DragHandle)
            .cursor(if axis == Axis::Horizontal {
                CursorIcon::ResizeColumn
            } else {
                CursorIcon::ResizeRow
            })
            .semantics(semantics)
            .on_action(CHANGED, move |_, a| {
                if let Some(v) = a.payload_value().and_then(|s| s.parse().ok()) {
                    on_change(v);
                }
            })
    })
}
pub fn scrollbar(
    rect: UiRect,
    axis: Axis,
    viewport: f32,
    content: f32,
    offset: impl Into<WidgetValue<f32>>,
) -> Element {
    let offset = offset.into();
    let value = offset.get();
    Element::new(move |cx| {
        let extent = finite(content).max(0.0);
        let view = finite(viewport).max(0.0);
        let max = (extent - view).max(0.0);
        let track = if axis == Axis::Horizontal {
            rect.width()
        } else {
            rect.height()
        }
        .max(0.0);
        let thumb = if extent > 0.0 {
            (track * view / extent).max(12.0).min(track)
        } else {
            track
        };
        let value = finite(value).clamp(0.0, max);
        cx.context.component_state_mut(&cx.id, |s: &mut DragState| {
            s.axis = axis;
            s.value = value;
            s.min = 0.0;
            s.max = max;
            s.scroll = true;
            s.track = track;
            s.thumb = thumb;
        });
        let start = if max > 0.0 {
            (track - thumb) * value / max
        } else {
            0.0
        };
        let thumb_rect = if axis == Axis::Horizontal {
            UiRect::new(
                rect.left + start,
                rect.top,
                rect.left + start + thumb,
                rect.bottom,
            )
        } else {
            UiRect::new(
                rect.left,
                rect.top + start,
                rect.right,
                rect.top + start + thumb,
            )
        };
        let tokens = cx
            .try_use_context::<ThemeContext>()
            .map(|t| *t.tokens())
            .unwrap_or_default();
        let mut semantics = Semantics::new(SemanticRole::Slider)
            .action(SemanticAction::Increment)
            .action(SemanticAction::Decrement)
            .action(SemanticAction::SetValue);
        semantics.numeric_value = Some(value as f64);
        semantics.numeric_min = Some(0.0);
        semantics.numeric_max = Some(max as f64);
        UiElement::group(cx.id.clone(), rect)
            .interaction(InteractionRole::DragHandle)
            .cursor(CursorIcon::Default)
            .semantics(semantics)
            .child(UiElement::panel(
                cx.auto_id(),
                thumb_rect,
                VisualStyle::filled(tokens.colors.text_muted).radius(tokens.radii.pill as f32),
            ))
            .on_action(CHANGED, move |_, a| {
                if let Some(v) = a.payload_value().and_then(|s| s.parse().ok()) {
                    offset.set(v);
                }
            })
    })
}
pub fn split(
    rect: UiRect,
    axis: Axis,
    ratio: impl Into<WidgetValue<f32>>,
    minimum: (f32, f32),
    first: impl FnOnce(UiRect) -> Element,
    second: impl FnOnce(UiRect) -> Element,
) -> Element {
    let ratio = ratio.into();
    let total = if axis == Axis::Horizontal {
        rect.width()
    } else {
        rect.height()
    }
    .max(0.0);
    let handle = 4.0_f32.min(total);
    let available = total - handle;
    let a = finite(minimum.0).max(0.0);
    let b = finite(minimum.1).max(0.0);
    let (lo, hi) = if a + b > available && a + b > 0.0 {
        let fixed = available * a / (a + b);
        (fixed, fixed)
    } else {
        (a, (available - b).max(a))
    };
    let cut = (available * finite(ratio.get()).clamp(0.0, 1.0)).clamp(lo, hi);
    let (r1, rh, r2) = if axis == Axis::Horizontal {
        (
            UiRect::new(rect.left, rect.top, rect.left + cut, rect.bottom),
            UiRect::new(
                rect.left + cut,
                rect.top,
                rect.left + cut + handle,
                rect.bottom,
            ),
            UiRect::new(rect.left + cut + handle, rect.top, rect.right, rect.bottom),
        )
    } else {
        (
            UiRect::new(rect.left, rect.top, rect.right, rect.top + cut),
            UiRect::new(
                rect.left,
                rect.top + cut,
                rect.right,
                rect.top + cut + handle,
            ),
            UiRect::new(rect.left, rect.top + cut + handle, rect.right, rect.bottom),
        )
    };
    let sizing = |extent| {
        if axis == Axis::Horizontal {
            SizeConstraints::new(Dimension::Points(extent), Dimension::Auto)
        } else {
            SizeConstraints::new(Dimension::Auto, Dimension::Points(extent))
        }
    };
    let mut layout = FlexLayout::new(axis);
    layout.align = Align::Stretch;
    group(rect)
        .layout(LayoutSpec::Flex(layout))
        .child(first(r1).size_constraints(sizing(cut)).flex_item(FlexItem {
            shrink: 0.0,
            ..Default::default()
        }))
        .child(
            resize_handle(rh, axis, cut, lo, hi, move |v| {
                ratio.set(if available > 0.0 { v / available } else { 0.0 })
            })
            .size_constraints(sizing(handle))
            .flex_item(FlexItem {
                shrink: 0.0,
                ..Default::default()
            }),
        )
        .child(
            second(r2)
                .size_constraints(sizing(available - cut))
                .flex_item(FlexItem {
                    shrink: 0.0,
                    ..Default::default()
                }),
        )
}

pub fn badge(rect: UiRect, label: impl Into<String>, color: Option<Color>) -> Element {
    let label = label.into();
    Element::new(move |cx| {
        let t = cx
            .try_use_context::<ThemeContext>()
            .map(|t| *t.tokens())
            .unwrap_or_default();
        UiElement::panel(
            cx.id.clone(),
            rect,
            VisualStyle::filled(color.unwrap_or(t.colors.accent)).radius(t.radii.pill as f32),
        )
        .semantics(Semantics::new(SemanticRole::Label).name(label.clone()))
        .child(UiElement::text(
            cx.auto_id(),
            rect,
            label,
            TextStyle::new(
                t.colors.accent_contrast,
                t.typography.label_size.abs() as f32,
                t.typography.strong_weight as i32,
            )
            .centered(),
        ))
    })
}
pub fn icon_button(rect: UiRect, key: impl Into<Arc<str>>, label: impl Into<String>) -> Element {
    let key = key.into();
    let label = label.into();
    Element::new(move |cx| {
        let t = cx
            .try_use_context::<ThemeContext>()
            .map(|t| *t.tokens())
            .unwrap_or_default();
        UiElement::button(
            cx.id.clone(),
            rect,
            VisualStyle::filled(t.colors.surface_interactive).radius(t.radii.small as f32),
        )
        .interaction(InteractionRole::Button)
        .semantics(
            Semantics::new(SemanticRole::Button)
                .name(label)
                .action(SemanticAction::Click),
        )
        .child(
            UiElement::icon(cx.auto_id(), rect.inset(EdgeInsets::all(4.0)), key)
                .icon_style(IconStyle::new(t.colors.text)),
        )
    })
}

#[derive(Clone, Default)]
struct RingState {
    phase: f32,
    active: bool,
}
impl ComponentState for RingState {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn advance(&mut self, ms: f32) -> bool {
        if self.active {
            self.phase = (self.phase + finite(ms).max(0.0) / 1000.0) % 1.0;
            true
        } else {
            false
        }
    }
    fn wants_frame(&self) -> bool {
        self.active
    }
}
pub fn progress_ring(rect: UiRect, value: Option<f32>, color: Color) -> Element {
    Element::new(move |cx| {
        let phase = cx.context.component_state_mut(&cx.id, |s: &mut RingState| {
            s.active = value.is_none();
            s.phase
        });
        if value.is_none() {
            cx.context.hook_updates().request_frame();
        }
        let center = Point::new(
            (rect.left + rect.right) / 2.0,
            (rect.top + rect.bottom) / 2.0,
        );
        let radius = (rect.width().min(rect.height()) / 2.0 - 1.0).max(0.0);
        let fraction = value.map_or(0.25, |v| finite(v).clamp(0.0, 1.0));
        let start = if value.is_none() {
            phase * std::f32::consts::TAU
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        let steps = (fraction * 64.0).ceil() as usize;
        let commands = (0..=steps).map(|i| {
            let angle = start + fraction * std::f32::consts::TAU * i as f32 / steps.max(1) as f32;
            let p = Point::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            );
            if i == 0 {
                UiPathCommand::MoveTo(p)
            } else {
                UiPathCommand::LineTo(p)
            }
        });
        let mut semantics = Semantics::new(SemanticRole::ProgressIndicator);
        semantics.numeric_value = value.map(|v| finite(v).clamp(0.0, 1.0) as f64);
        semantics.numeric_min = Some(0.0);
        semantics.numeric_max = Some(1.0);
        semantics.state.busy = value.is_none();
        let track = UiRect::new(
            center.x - radius,
            center.y - radius,
            center.x + radius,
            center.y + radius,
        );
        let mut root = UiElement::group(cx.id.clone(), rect)
            .semantics(semantics)
            .child(UiElement::ellipse(
                cx.auto_id(),
                track,
                VisualStyle::default().stroked(Stroke::new(color, 2.0, 48)),
            ));
        if fraction > 0.0 {
            root = root.child(UiElement::path(
                cx.auto_id(),
                rect,
                UiPath::new(commands),
                PathStyle {
                    fill: None,
                    fill_alpha: 0,
                    stroke: Some(Stroke::new(color, 2.0, 255)),
                },
            ));
        }
        root
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibilityTransition {
    pub duration_ms: u32,
    pub offset: Point,
}
impl Default for VisibilityTransition {
    fn default() -> Self {
        Self::from_theme(&crate::theme::ThemeTokens::default())
    }
}
impl VisibilityTransition {
    pub fn from_theme(tokens: &crate::theme::ThemeTokens) -> Self {
        Self {
            duration_ms: tokens.motion.normal_ms,
            offset: Point::new(0.0, tokens.spacing.sm as f32),
        }
    }
}
#[derive(Clone, Default)]
struct PresenceState {
    progress: f32,
    target: f32,
    duration: f32,
}
impl ComponentState for PresenceState {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn advance(&mut self, ms: f32) -> bool {
        let old = self.progress;
        if self.duration <= 0.0 {
            self.progress = self.target;
        } else {
            let amount = finite(ms).max(0.0) / self.duration;
            if self.target > self.progress {
                self.progress = (self.progress + amount).min(self.target);
            } else {
                self.progress = (self.progress - amount).max(self.target);
            }
        }
        old != self.progress
    }
    fn wants_frame(&self) -> bool {
        self.progress != self.target
    }
}
pub fn presence(
    rect: UiRect,
    visible: bool,
    transition: VisibilityTransition,
    child: impl Into<Element>,
) -> Element {
    Element::new(move |cx| {
        let progress = cx
            .context
            .component_state_mut(&cx.id, |s: &mut PresenceState| {
                s.target = if visible { 1.0 } else { 0.0 };
                s.duration = transition.duration_ms as f32;
                if s.duration == 0.0 {
                    s.progress = s.target;
                }
                s.progress
            });
        if progress != if visible { 1.0 } else { 0.0 } {
            cx.context.hook_updates().request_frame();
        }
        UiElement::compositing_layer(
            cx.id,
            rect,
            CompositingLayerSpec::new().opacity(progress).translation(
                transition.offset.x * (1.0 - progress),
                transition.offset.y * (1.0 - progress),
            ),
        )
        .visibility(if progress == 0.0 && !visible {
            Visibility::Collapsed
        } else {
            Visibility::Visible
        })
        .inert(!visible)
        .children(cx.children)
    })
    .child(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_constraints_and_scroll_thumb_offset_are_stable() {
        let mut s = DragState {
            value: 40.0,
            min: 20.0,
            max: 80.0,
            ..Default::default()
        };
        s.handle_action(&UiAction::new(POINTER_DOWN_ACTION).payload("5,0"));
        s.handle_action(&UiAction::new(POINTER_DRAG_ACTION).payload("100,0"));
        assert_eq!(s.value, 80.0);
        s.handle_action(&UiAction::new(POINTER_UP_ACTION));
        assert!(s.anchor.is_none());
        let mut s = DragState {
            scroll: true,
            track: 100.0,
            thumb: 20.0,
            max: 400.0,
            value: 100.0,
            ..Default::default()
        };
        s.handle_action(&UiAction::new(POINTER_DOWN_ACTION).payload("25,0"));
        s.handle_action(&UiAction::new(POINTER_DRAG_ACTION).payload("35,0"));
        assert_eq!(s.value, 150.0);
    }
    #[test]
    fn transitions_reverse_and_zero_duration_completes() {
        let mut p = PresenceState {
            target: 1.0,
            duration: 100.0,
            ..Default::default()
        };
        assert!(p.advance(40.0));
        assert!((p.progress - 0.4).abs() < 0.001);
        p.target = 0.0;
        p.advance(20.0);
        assert!((p.progress - 0.2).abs() < 0.001);
        p.duration = 0.0;
        p.advance(1.0);
        assert_eq!(p.progress, 0.0);
        assert!(!p.wants_frame());
    }
    #[test]
    fn determinate_progress_does_not_request_animation_frames() {
        let mut r = RingState::default();
        assert!(!r.advance(20.0));
        r.active = true;
        assert!(r.advance(100.0));
        assert!((r.phase - 0.1).abs() < 0.001);
    }
}
