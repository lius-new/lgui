//! A clipped region whose content scrolls by wheel or by dragging a thumb.
//!
//! The widget knows nothing about what it holds: the caller places the content
//! with its top-left at the region's top-left and says how large it is. When the
//! content is larger than the region along an enabled axis, the wheel moves it
//! and a thin thumb shows where the view is; the thumb can be dragged. The
//! offset lives in the widget, so callers keep no scroll state.
use std::any::Any;
use std::sync::{Arc, Mutex};

use crate::core::{
    Color, ComponentActionOutcome, ComponentState, CursorIcon, Element, ElementKey,
    ElementRenderCx, EventPolicy, InteractionRole, IntoElementContent, RenderPhase, UiAction,
    UiElement, UiEventKind, UiId, UiRect, VisualStyle, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION,
    POINTER_UP_ACTION, WHEEL_ACTION,
};

/// Pixels moved by one wheel line.
const LINE_PIXELS: f32 = 48.0;
const THUMB_THICKNESS: f32 = 3.0;
const THUMB_INSET: f32 = 2.0;
const THUMB_MIN: f32 = 16.0;
const THUMB_ALPHA: u8 = 0x99;
/// How far from the edge a thumb can be grabbed: wider than it is drawn.
const THUMB_GRAB: f32 = 12.0;
const SCROLL_ACTION: &str = "scroll_view.wheel";

pub struct ScrollView {
    key: ElementKey,
    rect: UiRect,
    content: (f32, f32),
    horizontal: bool,
    vertical: bool,
    thumb: Color,
    phase: RenderPhase,
    children: Vec<Element>,
}

impl ScrollView {
    /// Lets the content scroll sideways when it is wider than the region.
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// Lets the content scroll up and down when it is taller than the region.
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn thumb_color(mut self, color: Color) -> Self {
        self.thumb = color;
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
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

/// A region at `rect` over content of `content` (width, height), placed from the
/// region's top-left corner. Enable axes with [`ScrollView::horizontal`] and
/// [`ScrollView::vertical`]; with neither it only clips.
#[track_caller]
pub fn scroll_view(rect: UiRect, content: (f32, f32)) -> ScrollView {
    ScrollView {
        key: ElementKey::caller(),
        rect,
        content,
        horizontal: false,
        vertical: false,
        thumb: Color(0x808080),
        phase: RenderPhase::Content,
        children: Vec::new(),
    }
}

/// The thumb's length for a view of `view` over `content`.
fn thumb_length(view: f32, content: f32) -> f32 {
    (view * view / content).max(THUMB_MIN).min(view)
}

impl From<ScrollView> for Element {
    fn from(value: ScrollView) -> Self {
        let ScrollView {
            key,
            rect,
            content,
            horizontal,
            vertical,
            thumb,
            phase,
            children,
        } = value;
        Element::with_key(key, move |cx: ElementRenderCx<'_, '_, '_>| {
            let id = cx.id.clone();
            let max = (
                if horizontal {
                    (content.0 - rect.width()).max(0.0)
                } else {
                    0.0
                },
                if vertical {
                    (content.1 - rect.height()).max(0.0)
                } else {
                    0.0
                },
            );
            let (shared, offset_x, offset_y) =
                cx.context
                    .component_state_mut(&id, |state: &mut ScrollState| {
                        state.configure(max, vertical);
                        let offsets = *state.shared.lock().expect("scroll offsets poisoned");
                        (state.shared.clone(), offsets.x, offsets.y)
                    });
            // The clipped content takes the wheel, and hands it to this widget's state.
            let mut region = UiElement::clip(
                UiId::owned(format!("{}.content", id.as_str())),
                rect,
                -offset_x,
                -offset_y,
            )
            .render_phase(phase)
            .wheel_action(UiAction::new(SCROLL_ACTION))
            .action_target(id.clone())
            .children(cx.children);
            region = region.paint_bounds(rect);
            let mut root = UiElement::group(id.clone(), rect)
                .render_phase(phase)
                .child(region);
            let style = VisualStyle::filled(thumb)
                .alpha(THUMB_ALPHA)
                .radius(THUMB_THICKNESS / 2.0);
            // A thumb is its own target: it keeps the drag, and moves the shared offset.
            let add_thumb = |root: UiElement, name: &str, vertical_axis: bool| {
                let (view, extent, max, offset) = if vertical_axis {
                    (rect.height(), content.1, max.1, offset_y)
                } else {
                    (rect.width(), content.0, max.0, offset_x)
                };
                let length = thumb_length(view, extent);
                let travel = view - length;
                let along = travel * offset / max;
                let (drawn, grab) = if vertical_axis {
                    let top = rect.top + along;
                    let left = rect.right - THUMB_INSET - THUMB_THICKNESS;
                    (
                        UiRect::new(left, top, left + THUMB_THICKNESS, top + length),
                        UiRect::new(rect.right - THUMB_GRAB, top, rect.right, top + length),
                    )
                } else {
                    let left = rect.left + along;
                    let top = rect.bottom - THUMB_INSET - THUMB_THICKNESS;
                    (
                        UiRect::new(left, top, left + length, top + THUMB_THICKNESS),
                        UiRect::new(left, rect.bottom - THUMB_GRAB, left + length, rect.bottom),
                    )
                };
                let thumb_id = UiId::owned(format!("{}.{name}", id.as_str()));
                cx.context
                    .component_state_mut(&thumb_id, |state: &mut ThumbState| {
                        state.bind(shared.clone(), vertical_axis, max, travel);
                    });
                root.child(
                    UiElement::panel(thumb_id, drawn, style)
                        .render_phase(phase)
                        .hit_rect(grab)
                        .interaction(InteractionRole::Custom("scroll-thumb"))
                        .cursor(CursorIcon::Default)
                        .event_policy(EventPolicy {
                            hover: true,
                            press: true,
                            focus: false,
                        })
                        // The drag is the thumb's: what is behind must not also act on it.
                        .on_event(UiEventKind::PointerDown, |cx, _| cx.stop_propagation())
                        .on_event(UiEventKind::PointerDrag, |cx, _| cx.stop_propagation())
                        .on_event(UiEventKind::PointerUp, |cx, _| cx.stop_propagation()),
                )
            };
            if max.0 > 0.0 {
                root = add_thumb(root, "thumb-x", false);
            }
            if max.1 > 0.0 {
                root = add_thumb(root, "thumb-y", true);
            }
            root
        })
        .children(children)
    }
}

impl IntoElementContent for ScrollView {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Offsets {
    x: f32,
    y: f32,
}

type Shared = Arc<Mutex<Offsets>>;

/// The view's offsets, shared with its thumbs.
#[derive(Clone, Default)]
struct ScrollState {
    shared: Shared,
    max_x: f32,
    max_y: f32,
    vertical: bool,
}

impl ScrollState {
    fn configure(&mut self, max: (f32, f32), vertical: bool) {
        self.max_x = max.0;
        self.max_y = max.1;
        self.vertical = vertical;
        let mut offsets = self.shared.lock().expect("scroll offsets poisoned");
        offsets.x = offsets.x.clamp(0.0, self.max_x);
        offsets.y = offsets.y.clamp(0.0, self.max_y);
    }

    /// Applies a wheel turn of `x`,`y` pixels. With no vertical axis the
    /// vertical wheel moves the content sideways, as a lone scroll bar would.
    fn wheel(&mut self, x: f32, y: f32) -> bool {
        let (dx, dy) = if self.vertical { (x, y) } else { (x + y, 0.0) };
        let mut offsets = self.shared.lock().expect("scroll offsets poisoned");
        let next = Offsets {
            x: (offsets.x - dx).clamp(0.0, self.max_x),
            y: (offsets.y - dy).clamp(0.0, self.max_y),
        };
        let changed = next != *offsets;
        *offsets = next;
        changed
    }
}

impl ComponentState for ScrollState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
        if action.id().as_str() != WHEEL_ACTION {
            return ComponentActionOutcome::ignored();
        }
        let mut parts = action.payload_value().unwrap_or_default().split(',');
        let (Some(x), Some(y), Some(unit)) = (parts.next(), parts.next(), parts.next()) else {
            return ComponentActionOutcome::ignored();
        };
        let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) else {
            return ComponentActionOutcome::ignored();
        };
        let scale = if unit == "l" { LINE_PIXELS } else { 1.0 };
        if self.max_x <= 0.0 && self.max_y <= 0.0 {
            return ComponentActionOutcome::ignored();
        }
        ComponentActionOutcome::handled(self.wheel(x * scale, y * scale))
    }
}

/// One thumb being dragged. Pointer positions arrive relative to where the
/// thumb was when the press began, so a drag is the distance moved since.
#[derive(Clone, Default)]
struct ThumbState {
    shared: Shared,
    vertical: bool,
    max: f32,
    /// How far the thumb can travel along its track.
    travel: f32,
    /// The offset at the press and where on the thumb it was grabbed.
    drag: Option<(f32, f32)>,
}

impl ThumbState {
    fn bind(&mut self, shared: Shared, vertical: bool, max: f32, travel: f32) {
        self.shared = shared;
        self.vertical = vertical;
        self.max = max;
        self.travel = travel;
    }

    fn offset(&self) -> f32 {
        let offsets = self.shared.lock().expect("scroll offsets poisoned");
        if self.vertical {
            offsets.y
        } else {
            offsets.x
        }
    }

    fn position(&self, action: &UiAction) -> Option<f32> {
        let (x, y) = action.payload_value()?.split_once(',')?;
        let (x, y) = (x.parse::<f32>().ok()?, y.parse::<f32>().ok()?);
        Some(if self.vertical { y } else { x })
    }

    /// Moves the offset by what the pointer has travelled, in track pixels.
    fn follow(&mut self, position: f32) -> bool {
        let Some((start, grab)) = self.drag else {
            return false;
        };
        if self.travel <= 0.0 {
            return false;
        }
        let next = (start + (position - grab) * self.max / self.travel).clamp(0.0, self.max);
        let mut offsets = self.shared.lock().expect("scroll offsets poisoned");
        let current = if self.vertical {
            &mut offsets.y
        } else {
            &mut offsets.x
        };
        let changed = *current != next;
        *current = next;
        changed
    }
}

impl ComponentState for ThumbState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
        match action.id().as_str() {
            POINTER_DOWN_ACTION => {
                let Some(position) = self.position(action) else {
                    return ComponentActionOutcome::ignored();
                };
                self.drag = Some((self.offset(), position));
                ComponentActionOutcome::handled(false)
            }
            POINTER_DRAG_ACTION if self.drag.is_some() => {
                let Some(position) = self.position(action) else {
                    return ComponentActionOutcome::handled(false);
                };
                ComponentActionOutcome::handled(self.follow(position))
            }
            POINTER_UP_ACTION if self.drag.is_some() => {
                let changed = self
                    .position(action)
                    .is_some_and(|position| self.follow(position));
                self.drag = None;
                ComponentActionOutcome::handled(changed)
            }
            _ => ComponentActionOutcome::ignored(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(max: (f32, f32), vertical: bool) -> ScrollState {
        let mut state = ScrollState::default();
        state.configure(max, vertical);
        state
    }

    fn offsets(state: &ScrollState) -> (f32, f32) {
        let offsets = state.shared.lock().unwrap();
        (offsets.x, offsets.y)
    }

    fn turn(state: &mut ScrollState, payload: &str) -> ComponentActionOutcome {
        state.handle_action(&UiAction::new(WHEEL_ACTION).payload(payload.to_owned()))
    }

    #[test]
    fn the_vertical_wheel_scrolls_a_lone_horizontal_axis_and_clamps() {
        let mut state = state((100.0, 0.0), false);
        let down = turn(&mut state, "0,-1,l");
        assert!(down.handled && down.changed);
        assert_eq!(offsets(&state).0, LINE_PIXELS);
        turn(&mut state, "0,-5,l");
        assert_eq!(offsets(&state).0, 100.0, "clamped to the content's end");
        turn(&mut state, "0,10,l");
        assert_eq!(offsets(&state).0, 0.0, "clamped to the start");
    }

    #[test]
    fn with_both_axes_each_wheel_axis_moves_its_own() {
        let mut state = state((100.0, 100.0), true);
        turn(&mut state, "-10,-20,p");
        assert_eq!(offsets(&state), (10.0, 20.0));
    }

    #[test]
    fn content_that_fits_ignores_the_wheel() {
        let mut state = state((0.0, 0.0), false);
        assert!(!turn(&mut state, "0,-1,l").handled);
    }

    #[test]
    fn shrinking_content_pulls_the_offset_back() {
        let mut state = state((100.0, 0.0), false);
        turn(&mut state, "0,-5,l");
        state.configure((30.0, 0.0), false);
        assert_eq!(offsets(&state).0, 30.0);
    }

    #[test]
    fn dragging_a_thumb_moves_the_shared_offset_in_proportion() {
        let scroll = state((200.0, 0.0), false);
        let mut thumb = ThumbState::default();
        // The thumb can travel 100 along its track, for 200 of scrolling.
        thumb.bind(scroll.shared.clone(), false, 200.0, 100.0);
        let act = |thumb: &mut ThumbState, id: &'static str, payload: &str| {
            thumb.handle_action(&UiAction::new(id).payload(payload.to_owned()))
        };
        act(&mut thumb, POINTER_DOWN_ACTION, "5,2");
        let drag = act(&mut thumb, POINTER_DRAG_ACTION, "30,2");
        assert!(drag.changed);
        assert_eq!(offsets(&scroll).0, 50.0, "25 px of travel is half of 100");
        act(&mut thumb, POINTER_UP_ACTION, "205,2");
        assert_eq!(offsets(&scroll).0, 200.0, "past the track end clamps");
        let ignored = act(&mut thumb, POINTER_DRAG_ACTION, "0,0");
        assert!(!ignored.handled, "no drag after release");
    }
}
