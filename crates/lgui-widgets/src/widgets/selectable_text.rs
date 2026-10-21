//! Read-only text the pointer can select.
//!
//! The widget draws a list of positioned text segments in reading order, in
//! one surface. Pressing and dragging selects across segments; the selection is
//! highlighted and reported through [`SelectableText::on_select`] so the host can
//! copy it. It does not take keyboard focus, so it can sit in a popup over an
//! editor without stealing the editor's focus.
use std::any::Any;
use std::ops::Range;
use std::sync::Arc;

use lgui_core::text::{self, TextLayout, TextLayoutRequest};

use crate::core::{
    Color, ComponentActionOutcome, ComponentState, CursorIcon, Element, ElementKey,
    ElementRenderCx, EventPolicy, InteractionRole, UiEventKind, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION, POINTER_UP_ACTION,
    RenderPhase, TextStyle, UiAction, UiElement, UiEventContext, UiId, UiRect, VisualStyle,
};

const SELECTION_EVENT: &str = "selectable_text.select";
/// Gaps between segments on one line wider than this read as a space when copied.
const WORD_GAP: f32 = 1.0;
/// Default selection fill: the common translucent blue.
const DEFAULT_SELECTION: Color = Color(0x3390FF);
const SELECTION_ALPHA: u8 = 0x60;

/// Called with the selected text, or `None` when the selection is cleared.
pub type SelectHandler = Arc<dyn Fn(&mut UiEventContext, Option<String>) + Send + Sync>;

/// One piece of text at a fixed place. `rect` is where the text is drawn, and
/// should fit the text: it is also what the pointer is matched against.
#[derive(Clone, Debug)]
pub struct SelectableSegment {
    pub rect: UiRect,
    pub text: String,
    pub style: TextStyle,
}

impl SelectableSegment {
    pub fn new(rect: UiRect, text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            rect,
            text: text.into(),
            style,
        }
    }
}

pub struct SelectableText {
    key: ElementKey,
    rect: UiRect,
    segments: Vec<SelectableSegment>,
    selection_color: Color,
    on_select: Option<SelectHandler>,
    phase: RenderPhase,
}

impl SelectableText {
    pub fn selection_color(mut self, color: Color) -> Self {
        self.selection_color = color;
        self
    }

    pub fn on_select(
        mut self,
        handler: impl Fn(&mut UiEventContext, Option<String>) + Send + Sync + 'static,
    ) -> Self {
        self.on_select = Some(Arc::new(handler));
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = phase;
        self
    }
}

#[track_caller]
pub fn selectable_text(rect: UiRect, segments: Vec<SelectableSegment>) -> SelectableText {
    SelectableText {
        key: ElementKey::caller(),
        rect,
        segments,
        selection_color: DEFAULT_SELECTION,
        on_select: None,
        phase: RenderPhase::Content,
    }
}

impl From<SelectableText> for Element {
    fn from(value: SelectableText) -> Self {
        Element::with_key(value.key, move |cx: ElementRenderCx<'_, '_, '_>| {
            render(cx, value)
        })
    }
}

fn render(cx: ElementRenderCx<'_, '_, '_>, widget: SelectableText) -> UiElement {
    let id = cx.id.clone();
    let rect = widget.rect;
    let segments = relative(&widget.segments, rect);
    let highlights = cx
        .context
        .component_state_mut(&id, |state: &mut SelectableState| {
            state.configure(segments);
            state.highlights(&system_layout)
        });
    let mut root = UiElement::panel(id.clone(), rect, VisualStyle::default())
        .render_phase(widget.phase)
        .paint_bounds(rect)
        .hit_rect(rect)
        .interaction(InteractionRole::Custom("selectable-text"))
        .cursor(CursorIcon::Text)
        // A press, drag or release here is the selection's: what sits behind (an editor
        // that would move its caret) must not also act on it.
        .on_event(UiEventKind::PointerDown, |cx, _| cx.stop_propagation())
        .on_event(UiEventKind::PointerDrag, |cx, _| cx.stop_propagation())
        .on_event(UiEventKind::PointerUp, |cx, _| cx.stop_propagation())
        .event_policy(EventPolicy {
            hover: true,
            press: true,
            focus: false,
        });
    for (index, highlight) in highlights.into_iter().enumerate() {
        root = root.child(
            UiElement::panel(
                UiId::owned(format!("{}.sel{index}", id.as_str())),
                UiRect::new(
                    rect.left + highlight.left,
                    rect.top + highlight.top,
                    rect.left + highlight.right,
                    rect.top + highlight.bottom,
                ),
                VisualStyle::filled(widget.selection_color).alpha(SELECTION_ALPHA),
            )
            .render_phase(widget.phase),
        );
    }
    for (index, segment) in widget.segments.into_iter().enumerate() {
        root = root.child(
            UiElement::text(
                UiId::owned(format!("{}.text{index}", id.as_str())),
                segment.rect,
                segment.text,
                segment.style,
            )
            .render_phase(widget.phase),
        );
    }
    if let Some(on_select) = widget.on_select {
        root = root.on_action(SELECTION_EVENT, move |context, action| {
            on_select(
                context,
                action
                    .payload_value()
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned),
            );
        });
    }
    root
}

/// The segments with their rectangles measured from the widget's corner, so the
/// state is independent of where the widget is scrolled or translated to.
fn relative(segments: &[SelectableSegment], origin: UiRect) -> Vec<SelectableSegment> {
    segments
        .iter()
        .map(|segment| SelectableSegment {
            rect: UiRect::new(
                segment.rect.left - origin.left,
                segment.rect.top - origin.top,
                segment.rect.right - origin.left,
                segment.rect.bottom - origin.top,
            ),
            text: segment.text.clone(),
            style: segment.style,
        })
        .collect()
}

/// The same layout the renderer builds for a segment, with its rect at the
/// origin: positions inside it are relative to the segment's corner.
fn system_layout(segment: &SelectableSegment) -> Option<TextLayout> {
    let style = segment.style;
    let bounds = UiRect::new(0.0, 0.0, segment.rect.width(), segment.rect.height());
    let mut request =
        TextLayoutRequest::single_line(&segment.text, bounds, style.height, style.weight);
    request.font_slant = style.font_slant;
    request.tracking = style.tracking;
    request.align = style.align;
    request.vertical_align = style.vertical_align;
    request.font_families = style.font_families;
    text::layout(&request)
}

/// A place in the text: a segment and a byte offset into it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    segment: usize,
    offset: usize,
}

#[derive(Clone, Default)]
struct SelectableState {
    segments: Vec<SelectableSegment>,
    anchor: Option<Position>,
    focus: Option<Position>,
    dragging: bool,
    /// Whether the host was told about a selection it has not been told is gone.
    reported: bool,
}

type LayoutOf<'a> = &'a dyn Fn(&SelectableSegment) -> Option<TextLayout>;

impl SelectableState {
    fn configure(&mut self, segments: Vec<SelectableSegment>) {
        let changed = self.segments.len() != segments.len()
            || self
                .segments
                .iter()
                .zip(&segments)
                .any(|(old, new)| old.text != new.text);
        if changed {
            // The text moved under the selection: it no longer means anything.
            self.anchor = None;
            self.focus = None;
            self.dragging = false;
        }
        self.segments = segments;
    }

    /// The ordered ends of the selection, if there is a non-empty one.
    fn span(&self) -> Option<(Position, Position)> {
        let (anchor, focus) = (self.anchor?, self.focus?);
        (anchor != focus).then(|| (anchor.min(focus), anchor.max(focus)))
    }

    /// The position nearest `(x, y)`, from the layouts of the nearest segment.
    fn position_at(&self, x: f32, y: f32, layout: LayoutOf<'_>) -> Option<Position> {
        let (index, segment) = self.segments.iter().enumerate().min_by(|(_, a), (_, b)| {
            // Segments may overlap a little (text rects carry a clipping margin):
            // among equals the nearer centre wins.
            let near = |segment: &SelectableSegment| {
                (
                    axis_distance(y, segment.rect.top, segment.rect.bottom),
                    axis_distance(x, segment.rect.left, segment.rect.right),
                    (x - (segment.rect.left + segment.rect.right) / 2.0).abs(),
                )
            };
            let (a, b) = (near(a), near(b));
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
        })?;
        let hit = layout(segment)?.hit_test(x - segment.rect.left, y - segment.rect.top);
        Some(Position {
            segment: index,
            offset: hit.index.min(segment.text.len()),
        })
    }

    /// The selected part of segment `index`, as byte offsets.
    fn range_in(&self, index: usize) -> Option<Range<usize>> {
        let (start, end) = self.span()?;
        if index < start.segment || index > end.segment {
            return None;
        }
        let len = self.segments[index].text.len();
        let from = if index == start.segment {
            start.offset
        } else {
            0
        };
        let to = if index == end.segment {
            end.offset
        } else {
            len
        };
        (from < to).then_some(from..to)
    }

    /// The selected text, joined like it reads: a space across a gap on one
    /// line and a newline between lines.
    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.span()?;
        let mut out = String::new();
        let mut previous: Option<&SelectableSegment> = None;
        for index in start.segment..=end.segment {
            let Some(range) = self.range_in(index) else {
                continue;
            };
            let segment = &self.segments[index];
            if let Some(before) = previous {
                if segment.rect.top >= before.rect.bottom - 0.5 {
                    out.push('\n');
                } else if segment.rect.left - before.rect.right > WORD_GAP {
                    out.push(' ');
                }
            }
            out.push_str(segment.text.get(range)?);
            previous = Some(segment);
        }
        (!out.is_empty()).then_some(out)
    }

    /// Rectangles to fill behind the selected text, relative to the widget.
    fn highlights(&self, layout: LayoutOf<'_>) -> Vec<UiRect> {
        let mut rects = Vec::new();
        let Some((start, end)) = self.span() else {
            return rects;
        };
        for index in start.segment..=end.segment {
            let Some(range) = self.range_in(index) else {
                continue;
            };
            let segment = &self.segments[index];
            let Some(laid_out) = layout(segment) else {
                continue;
            };
            for local in laid_out.selection_rects(range.clone()) {
                rects.push(UiRect::new(
                    segment.rect.left + local.left,
                    segment.rect.top + local.top,
                    segment.rect.left + local.right,
                    segment.rect.top + local.bottom,
                ));
            }
            // The space between two selected words on a line is selected too.
            if range.end == segment.text.len()
                && index < end.segment
                && let Some(next) = self.segments.get(index + 1)
                && self.range_in(index + 1).is_some()
                && next.rect.top < segment.rect.bottom - 0.5
                && next.rect.left > segment.rect.right
            {
                rects.push(UiRect::new(
                    segment.rect.right,
                    segment.rect.top.min(next.rect.top),
                    next.rect.left,
                    segment.rect.bottom.max(next.rect.bottom),
                ));
            }
        }
        rects
    }

    /// What the host should hear after the selection changed.
    fn report(&mut self) -> ComponentActionOutcome {
        let text = self.selected_text();
        let had = std::mem::replace(&mut self.reported, text.is_some());
        let outcome = ComponentActionOutcome::handled(true);
        match text {
            Some(text) => outcome.emit(UiAction::new(SELECTION_EVENT).payload(text)),
            None if had => outcome.emit(UiAction::new(SELECTION_EVENT).payload("")),
            None => outcome,
        }
    }

    fn pointer(&mut self, action: &UiAction, layout: LayoutOf<'_>) -> Option<(f32, f32, Position)> {
        let (x, y) = action.payload_value()?.split_once(',')?;
        let (x, y) = (x.parse::<f32>().ok()?, y.parse::<f32>().ok()?);
        Some((x, y, self.position_at(x, y, layout)?))
    }

    fn handle(&mut self, action: &UiAction, layout: LayoutOf<'_>) -> ComponentActionOutcome {
        match action.id().as_str() {
            POINTER_DOWN_ACTION => {
                let Some((_, _, position)) = self.pointer(action, layout) else {
                    return ComponentActionOutcome::ignored();
                };
                self.anchor = Some(position);
                self.focus = Some(position);
                self.dragging = true;
                self.report()
            }
            POINTER_DRAG_ACTION if self.dragging => {
                let Some((_, _, position)) = self.pointer(action, layout) else {
                    return ComponentActionOutcome::handled(false);
                };
                if self.focus == Some(position) {
                    return ComponentActionOutcome::handled(false);
                }
                self.focus = Some(position);
                // Reported as it grows: the host redraws from what it is told, so
                // the highlight follows the pointer.
                self.report()
            }
            POINTER_UP_ACTION if self.dragging => {
                if let Some((_, _, position)) = self.pointer(action, layout) {
                    self.focus = Some(position);
                }
                self.dragging = false;
                self.report()
            }
            _ => ComponentActionOutcome::ignored(),
        }
    }
}

fn axis_distance(value: f32, start: f32, end: f32) -> f32 {
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        0.0
    }
}

impl ComponentState for SelectableState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
        self.handle(action, &system_layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lgui_core::text::{TextAffinity, TextCluster, TextLineMetrics};

    /// Ten units per byte, one cluster per byte, left aligned.
    fn fake(segment: &SelectableSegment) -> Option<TextLayout> {
        let height = segment.rect.height();
        let clusters: Vec<TextCluster> = (0..segment.text.len())
            .map(|i| TextCluster {
                range: i..i + 1,
                bounds: UiRect::new(i as f32 * 10.0, 0.0, i as f32 * 10.0 + 10.0, height),
                direction: Default::default(),
            })
            .collect();
        let width = segment.text.len() as f32 * 10.0;
        let line = TextLineMetrics {
            range: 0..segment.text.len(),
            bounds: UiRect::new(0.0, 0.0, width, height),
            baseline: height,
            hard_break: false,
        };
        let carets = (0..=segment.text.len())
            .map(|i| {
                (
                    i,
                    TextAffinity::Downstream,
                    UiRect::new(i as f32 * 10.0, 0.0, i as f32 * 10.0 + 1.0, height),
                )
            })
            .collect();
        Some(TextLayout::new(
            width,
            height,
            false,
            vec![line],
            clusters,
            carets,
        ))
    }

    fn state() -> SelectableState {
        let style = TextStyle::new(Color(0), 14.0, 400);
        let mut state = SelectableState::default();
        // "hello world" on line one (two words), "again" on line two.
        state.configure(vec![
            SelectableSegment::new(UiRect::new(0.0, 0.0, 50.0, 20.0), "hello", style),
            SelectableSegment::new(UiRect::new(60.0, 0.0, 110.0, 20.0), "world", style),
            SelectableSegment::new(UiRect::new(0.0, 20.0, 50.0, 40.0), "again", style),
        ]);
        state
    }

    fn press(
        state: &mut SelectableState,
        id: &'static str,
        x: f32,
        y: f32,
    ) -> ComponentActionOutcome {
        state.handle(&UiAction::new(id).payload(format!("{x},{y}")), &fake)
    }

    #[test]
    fn dragging_across_words_selects_and_reports_joined_text_on_release() {
        let mut state = state();
        let down = press(&mut state, POINTER_DOWN_ACTION, 22.0, 5.0);
        assert!(down.events.is_empty(), "a press alone selects nothing");
        let drag = press(&mut state, POINTER_DRAG_ACTION, 82.0, 5.0);
        assert!(drag.changed);
        assert_eq!(drag.events[0].payload_value(), Some("llo wo"), "live while dragging");
        let up = press(&mut state, POINTER_UP_ACTION, 82.0, 5.0);
        assert_eq!(up.events.len(), 1);
        // From the middle of "hello" to the middle of "world", with the gap as a space.
        assert_eq!(up.events[0].payload_value(), Some("llo wo"));
    }

    #[test]
    fn a_selection_across_lines_joins_with_a_newline() {
        let mut state = state();
        press(&mut state, POINTER_DOWN_ACTION, 82.0, 5.0);
        let up = press(&mut state, POINTER_UP_ACTION, 22.0, 30.0);
        assert_eq!(up.events[0].payload_value(), Some("rld\nag"));
    }

    #[test]
    fn dragging_backwards_selects_the_same_text() {
        let mut state = state();
        press(&mut state, POINTER_DOWN_ACTION, 82.0, 5.0);
        let up = press(&mut state, POINTER_UP_ACTION, 22.0, 5.0);
        assert_eq!(up.events[0].payload_value(), Some("llo wo"));
    }

    #[test]
    fn a_plain_click_clears_the_selection_and_tells_the_host_once() {
        let mut state = state();
        press(&mut state, POINTER_DOWN_ACTION, 2.0, 5.0);
        press(&mut state, POINTER_UP_ACTION, 82.0, 5.0);
        assert!(state.selected_text().is_some());
        let down = press(&mut state, POINTER_DOWN_ACTION, 22.0, 5.0);
        assert_eq!(down.events[0].payload_value(), Some(""));
        let up = press(&mut state, POINTER_UP_ACTION, 22.0, 5.0);
        assert!(up.events.is_empty(), "already cleared: nothing new to say");
        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn highlights_cover_the_selection_and_the_space_between_words() {
        let mut state = state();
        press(&mut state, POINTER_DOWN_ACTION, 22.0, 5.0);
        press(&mut state, POINTER_UP_ACTION, 82.0, 5.0);
        let rects = state.highlights(&fake);
        let left = rects.iter().map(|r| r.left).fold(f32::MAX, f32::min);
        let right = rects.iter().map(|r| r.right).fold(f32::MIN, f32::max);
        assert_eq!((left, right), (20.0, 80.0));
        assert!(
            rects.iter().any(|r| r.left == 50.0 && r.right == 60.0),
            "the gap between the words is filled"
        );
    }

    #[test]
    fn changed_text_drops_the_selection() {
        let mut state = state();
        press(&mut state, POINTER_DOWN_ACTION, 2.0, 5.0);
        press(&mut state, POINTER_UP_ACTION, 82.0, 5.0);
        let style = TextStyle::new(Color(0), 14.0, 400);
        state.configure(vec![SelectableSegment::new(
            UiRect::new(0.0, 0.0, 50.0, 20.0),
            "other",
            style,
        )]);
        assert_eq!(state.selected_text(), None);
    }
}
