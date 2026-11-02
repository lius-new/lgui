//! Portable editing state and a renderer-measured single/multiline text control.
use crate::WidgetValue;
use crate::core::*;
use crate::theme::ThemeContext;
use lgui_core::text::{TextLayout, TextLayoutRequest};
use std::{
    any::Any,
    ops::Range,
    sync::{Arc, Mutex},
};
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TextInputModel {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub focused: bool,
    pub preedit: String,
    pub preedit_cursor: Option<Range<usize>>,
    pub scroll: Point,
    undo: Vec<(String, usize, usize)>,
    redo: Vec<(String, usize, usize)>,
}
impl TextInputModel {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            anchor: cursor,
            ..Default::default()
        }
    }
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
    fn boundary(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(self.text.len()))
            .take_while(|i| *i <= index)
            .last()
            .unwrap_or(0)
    }
    pub fn select(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.boundary(anchor);
        self.cursor = self.boundary(cursor);
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }
    fn remember(&mut self) {
        self.undo
            .push((self.text.clone(), self.anchor, self.cursor));
        self.redo.clear();
    }
    pub fn replace_selection(&mut self, text: &str) {
        self.select(self.anchor, self.cursor);
        if text.is_empty() && self.selection().is_empty() {
            return;
        }
        self.remember();
        let r = self.selection();
        self.text.replace_range(r.clone(), text);
        self.cursor = r.start + text.len();
        self.anchor = self.cursor;
        self.preedit.clear();
        self.preedit_cursor = None;
    }
    pub fn backspace(&mut self) {
        self.select(self.anchor, self.cursor);
        if self.selection().is_empty() {
            self.anchor = self.text[..self.cursor]
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(i, _)| i);
        }
        if !self.selection().is_empty() {
            self.replace_selection("");
        }
    }
    pub fn delete_forward(&mut self) {
        self.select(self.anchor, self.cursor);
        if self.selection().is_empty() {
            self.anchor = self.text[self.cursor..]
                .graphemes(true)
                .next()
                .map_or(self.cursor, |s| self.cursor + s.len());
        }
        if !self.selection().is_empty() {
            self.replace_selection("");
        }
    }
    pub fn move_cursor(&mut self, right: bool, extend: bool) {
        self.select(self.anchor, self.cursor);
        let to = if !extend && !self.selection().is_empty() {
            if right {
                self.selection().end
            } else {
                self.selection().start
            }
        } else if right {
            self.text[self.cursor..]
                .graphemes(true)
                .next()
                .map_or(self.cursor, |s| self.cursor + s.len())
        } else {
            self.text[..self.cursor]
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(i, _)| i)
        };
        self.cursor = to;
        if !extend {
            self.anchor = to;
        }
    }
    pub fn move_vertical(&mut self, down: bool, extend: bool) {
        self.select(self.anchor, self.cursor);
        let start = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1);
        let col = self.text[start..self.cursor].graphemes(true).count();
        let next = if down {
            self.text[self.cursor..]
                .find('\n')
                .map(|i| self.cursor + i + 1)
        } else if start > 0 {
            Some(self.text[..start - 1].rfind('\n').map_or(0, |i| i + 1))
        } else {
            None
        };
        if let Some(next) = next {
            let end = self.text[next..]
                .find('\n')
                .map_or(self.text.len(), |i| next + i);
            self.cursor = self.text[next..end]
                .grapheme_indices(true)
                .nth(col)
                .map_or(end, |(i, _)| next + i);
            if !extend {
                self.anchor = self.cursor;
            }
        }
    }
    pub fn move_word(&mut self, right: bool, extend: bool) {
        self.select(self.anchor, self.cursor);
        self.cursor = if right {
            self.text[self.cursor..]
                .unicode_word_indices()
                .find(|(i, _)| *i > 0)
                .map_or(self.text.len(), |(i, _)| self.cursor + i)
        } else {
            self.text[..self.cursor]
                .unicode_word_indices()
                .next_back()
                .map_or(0, |(i, _)| i)
        };
        if !extend {
            self.anchor = self.cursor;
        }
    }
    pub fn undo(&mut self) {
        if let Some((text, a, c)) = self.undo.pop() {
            self.redo
                .push((self.text.clone(), self.anchor, self.cursor));
            self.text = text;
            self.anchor = a;
            self.cursor = c;
            self.preedit.clear();
        }
    }
    pub fn redo(&mut self) {
        if let Some((text, a, c)) = self.redo.pop() {
            self.undo
                .push((self.text.clone(), self.anchor, self.cursor));
            self.text = text;
            self.anchor = a;
            self.cursor = c;
            self.preedit.clear();
        }
    }
}
#[derive(Clone, Default)]
pub struct TextClipboard {
    pub read: Option<Arc<dyn Fn() -> String + Send + Sync>>,
    pub write: Option<Arc<dyn Fn(String) + Send + Sync>>,
}
type Changed = Arc<dyn Fn(String) + Send + Sync>;
type Selected = Arc<dyn Fn(Range<usize>) + Send + Sync>;
type Identified = Arc<dyn Fn(IdentifiedEvent) + Send + Sync>;
#[derive(Clone, Default)]
struct InputActions;
impl ComponentState for InputActions {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
        let id = match action.id().as_str() {
            "semantic.set_value" => "input.set_value",
            POINTER_DOWN_ACTION => "input.pointer_down",
            POINTER_DRAG_ACTION => "input.pointer_drag",
            _ => return ComponentActionOutcome::ignored(),
        };
        ComponentActionOutcome::handled(false)
            .emit(UiAction::new(id).payload(action.payload_value().unwrap_or_default().to_owned()))
    }
}
pub struct TextInput {
    rect: UiRect,
    model: WidgetValue<TextInputModel>,
    multiline: bool,
    read_only: bool,
    password: bool,
    placeholder: String,
    clipboard: TextClipboard,
    changed: Option<Changed>,
    selected: Option<Selected>,
    style: Option<(VisualStyle, TextStyle)>,
    public_id: Option<Arc<str>>,
    identified: Option<Identified>,
    identity: Option<UiId>,
    label: Option<String>,
}
pub fn text_input(rect: UiRect, model: impl Into<WidgetValue<TextInputModel>>) -> TextInput {
    let rect = UiRect::new(
        rect.left,
        rect.top,
        rect.right.max(rect.left),
        rect.bottom.max(rect.top),
    );
    TextInput {
        rect,
        model: model.into(),
        multiline: false,
        read_only: false,
        password: false,
        placeholder: String::new(),
        clipboard: TextClipboard::default(),
        changed: None,
        selected: None,
        style: None,
        public_id: None,
        identified: None,
        identity: None,
        label: None,
    }
}
pub fn multiline_input(rect: UiRect, model: impl Into<WidgetValue<TextInputModel>>) -> TextInput {
    text_input(rect, model).multiline(true)
}
impl TextInput {
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    /// Reuses an existing native focus target during an application's migration.
    pub fn id(mut self, id: UiId) -> Self {
        self.identity = Some(id);
        self
    }
    pub fn public_id(mut self, id: impl Into<Arc<str>>) -> Self {
        self.public_id = Some(id.into());
        self
    }
    /// Receives committed value/selection changes with the control's public ID.
    /// Raw pointer, keyboard and input events use RuntimeOutput::identified_events.
    pub fn on_identified_event(
        mut self,
        f: impl Fn(IdentifiedEvent) + Send + Sync + 'static,
    ) -> Self {
        self.identified = Some(Arc::new(f));
        self
    }
    pub fn multiline(mut self, v: bool) -> Self {
        self.multiline = v;
        self
    }
    pub fn read_only(mut self, v: bool) -> Self {
        self.read_only = v;
        self
    }
    pub fn password(mut self, v: bool) -> Self {
        self.password = v;
        self
    }
    pub fn placeholder(mut self, v: impl Into<String>) -> Self {
        self.placeholder = v.into();
        self
    }
    pub fn clipboard(mut self, v: TextClipboard) -> Self {
        self.clipboard = v;
        self
    }
    pub fn on_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.changed = Some(Arc::new(f));
        self
    }
    pub fn on_selection(mut self, f: impl Fn(Range<usize>) + Send + Sync + 'static) -> Self {
        self.selected = Some(Arc::new(f));
        self
    }
    pub fn style(mut self, panel: VisualStyle, text: TextStyle) -> Self {
        self.style = Some((panel, text));
        self
    }
}
fn commit(
    state: &WidgetValue<TextInputModel>,
    before: TextInputModel,
    mut next: TextInputModel,
    changed: &Option<Changed>,
    selected: &Option<Selected>,
) {
    next.select(next.anchor, next.cursor);
    if before != next {
        if before.text != next.text
            && let Some(f) = changed
        {
            f(next.text.clone());
        }
        if before.selection() != next.selection()
            && let Some(f) = selected
        {
            f(next.selection());
        }
        state.set(next);
    }
}
fn composed(model: &TextInputModel) -> String {
    let mut text = model.text.clone();
    if !model.preedit.is_empty() {
        text.replace_range(model.selection(), &model.preedit);
    }
    text
}
fn display_cursor(model: &TextInputModel, password: bool) -> usize {
    let full = composed(model);
    let mut index = if model.preedit.is_empty() {
        model.cursor
    } else {
        model.selection().start
            + model
                .preedit_cursor
                .as_ref()
                .map_or(model.preedit.len(), |r| r.end.min(model.preedit.len()))
    };
    index = index.min(full.len());
    while !full.is_char_boundary(index) {
        index -= 1;
    }
    if password {
        full[..index].graphemes(true).count()
    } else {
        full[..index].chars().count()
    }
}
#[derive(Clone)]
struct InputGeometry {
    inner: UiRect,
    style: TextStyle,
    layout: Option<TextLayout>,
}
fn select_at(
    state: &WidgetValue<TextInputModel>,
    geometry: &Mutex<InputGeometry>,
    action: &UiAction,
    origin: UiRect,
    extend: bool,
    password: bool,
    selected: &Option<Selected>,
) {
    let Some((x, y)) = action.payload_value().and_then(|s| s.split_once(',')) else {
        return;
    };
    let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) else {
        return;
    };
    let before = state.get();
    let mut next = before.clone();
    if let Some(layout) = &geometry.lock().expect("input geometry poisoned").layout {
        // Default actions carry coordinates relative to the actual hit rectangle,
        // including layout/anchor/scroll translations, rather than declared bounds.
        let mut index = layout.hit_test(origin.left + x, origin.top + y).index;
        if password {
            index = next
                .text
                .grapheme_indices(true)
                .nth(index)
                .map_or(next.text.len(), |(i, _)| i);
        } else {
            index = next
                .text
                .char_indices()
                .nth(index)
                .map_or(next.text.len(), |(i, _)| i);
        }
        next.select(if extend { next.anchor } else { index }, index);
    }
    commit(state, before, next, &None, selected);
}
fn move_visual(
    model: &mut TextInputModel,
    geo: &InputGeometry,
    down: bool,
    extend: bool,
    password: bool,
) {
    if let Some(layout) = &geo.layout
        && let Some(caret) = layout.caret_rect(display_cursor(model, password))
    {
        let y = (caret.top + caret.bottom) / 2.0
            + if down {
                caret.height()
            } else {
                -caret.height()
            };
        let index = layout.hit_test(caret.left, y).index;
        let byte = if password {
            model
                .text
                .grapheme_indices(true)
                .nth(index)
                .map_or(model.text.len(), |(i, _)| i)
        } else {
            model
                .text
                .char_indices()
                .nth(index)
                .map_or(model.text.len(), |(i, _)| i)
        };
        model.select(if extend { model.anchor } else { byte }, byte);
    } else {
        model.move_vertical(down, extend);
    }
}
fn keep_visible(model: &mut TextInputModel, geo: &InputGeometry, multiline: bool, password: bool) {
    let bounds = geo.inner.translate(-model.scroll.x, -model.scroll.y);
    if let Some(l) = layout(&display(model, password), bounds, geo.style, multiline) {
        if let Some(c) = l.caret_rect(display_cursor(model, password)) {
            if c.left < geo.inner.left {
                model.scroll.x = (model.scroll.x + c.left - geo.inner.left).max(0.0);
            } else if c.right > geo.inner.right {
                model.scroll.x += c.right - geo.inner.right;
            }
            if multiline {
                if c.top < geo.inner.top {
                    model.scroll.y = (model.scroll.y + c.top - geo.inner.top).max(0.0);
                } else if c.bottom > geo.inner.bottom {
                    model.scroll.y += c.bottom - geo.inner.bottom;
                }
            }
        }
        if multiline {
            model.scroll.x = 0.0;
            model.scroll.y = model
                .scroll
                .y
                .clamp(0.0, (l.height - geo.inner.height()).max(0.0));
        } else {
            model.scroll.y = 0.0;
            model.scroll.x = model
                .scroll
                .x
                .clamp(0.0, (l.width - geo.inner.width()).max(0.0));
        }
    }
}
fn display(model: &TextInputModel, password: bool) -> String {
    let mut text = model.text.clone();
    if !model.preedit.is_empty() {
        text.replace_range(model.selection(), &model.preedit);
    }
    if password {
        "•".repeat(text.graphemes(true).count())
    } else {
        text
    }
}
fn layout(value: &str, bounds: UiRect, style: TextStyle, multiline: bool) -> Option<TextLayout> {
    let mut request = TextLayoutRequest::for_style(value, bounds, &style);
    if multiline {
        request.max_lines = None;
    } else {
        request.bounds.right = request.bounds.left + 1_000_000.0;
    }
    lgui_core::text::layout(&request)
}
impl From<TextInput> for Element {
    fn from(w: TextInput) -> Self {
        let TextInput {
            rect,
            model,
            multiline,
            read_only,
            password,
            placeholder,
            clipboard,
            changed,
            selected,
            style,
            public_id,
            identified,
            identity,
            label,
        } = w;
        let changes = identified.clone();
        let name = public_id.clone();
        let changed: Option<Changed> = Some(Arc::new(move |value| {
            if let Some(f) = &changed {
                f(value.clone());
            }
            if let (Some(f), Some(name)) = (&changes, &name) {
                f(IdentifiedEvent {
                    node: name.clone(),
                    payload: UiEventPayload::Change { value: Some(value) },
                });
            }
        }));
        let name = public_id.clone();
        let selected: Option<Selected> = Some(Arc::new(move |range| {
            if let Some(f) = &selected {
                f(range.clone());
            }
            if let (Some(f), Some(name)) = (&identified, &name) {
                f(IdentifiedEvent {
                    node: name.clone(),
                    payload: UiEventPayload::Selection { range },
                });
            }
        }));
        let drawing = model.clone();
        let selection_model = model.clone();
        let dragging = model.clone();
        let key_model = model.clone();
        let composing = model.clone();
        let focus_model = model.clone();
        let blur_model = model.clone();
        let on_edit = changed.clone();
        let on_select = selected.clone();
        let key_changed = changed.clone();
        let key_selected = selected.clone();
        let click_selected = selected.clone();
        let geometry = Arc::new(Mutex::new(InputGeometry {
            inner: rect.inset(EdgeInsets::all(6.0)),
            style: style.map_or(TextStyle::new(Color::WHITE, 14.0, 400), |(_, s)| s),
            layout: None,
        }));
        let paint_geometry = geometry.clone();
        let edit_geometry = geometry.clone();
        let key_geometry = geometry.clone();
        let mouse_geometry = geometry.clone();
        let drag_geometry = geometry.clone();
        let composition_geometry = geometry.clone();
        let wheel_geometry = geometry.clone();
        let focus_geometry = geometry;
        let scrolling = model.clone();
        let semantic_model = model.clone();
        let semantic_changed = changed.clone();
        let semantic_selected = selected.clone();
        let element = Element::new(move |cx| {
            let id = identity.unwrap_or_else(|| cx.id.clone());
            cx.context
                .component_state_mut(&id, |_: &mut InputActions| {});
            let tokens = cx
                .try_use_context::<ThemeContext>()
                .map(|t| *t.tokens())
                .unwrap_or_default();
            let default = TextStyle::new(
                tokens.colors.text,
                tokens.typography.body_size.abs() as f32,
                tokens.typography.body_weight as i32,
            );
            let (panel, mut text_style) = style.unwrap_or((
                VisualStyle::filled(tokens.colors.surface_interactive)
                    .radius(tokens.radii.medium as f32),
                default,
            ));
            if multiline {
                text_style = text_style
                    .wrap(TextWrap::Words)
                    .vertical_align(lgui_core::text::TextVerticalAlign::Top);
            }
            let padding = (tokens.spacing.sm.max(0) as f32)
                .min(rect.width().max(0.0) / 2.0)
                .min(rect.height().max(0.0) / 2.0);
            let inner = rect.inset(EdgeInsets::all(padding));
            let mut m = drawing.get();
            m.select(m.anchor, m.cursor);
            let value = display(&m, password);
            let mut semantics = Semantics::new(if password {
                SemanticRole::PasswordInput
            } else {
                SemanticRole::TextInput
            })
            .value(if password {
                "•".repeat(m.text.graphemes(true).count())
            } else {
                m.text.clone()
            })
            .action(SemanticAction::SetValue);
            semantics.state.read_only = read_only;
            semantics.name = label;
            semantics.state.multiline = multiline;
            semantics.state.password = password;
            semantics.text.selection = Some(
                m.text[..m.selection().start].chars().count()
                    ..m.text[..m.selection().end].chars().count(),
            );
            semantics.text.character_count = Some(m.text.chars().count());
            let mut root = UiElement::panel(id, rect, panel)
                .interaction(InteractionRole::Custom("text-input"))
                .cursor(CursorIcon::Text)
                .semantics(semantics)
                .event_policy(EventPolicy {
                    hover: true,
                    press: true,
                    focus: true,
                });
            let bounds = inner.translate(-m.scroll.x, -m.scroll.y);
            let mut content = UiElement::clip(cx.auto_id(), inner, 0.0, 0.0);
            let measured = layout(&value, bounds, text_style, multiline);
            *paint_geometry.lock().expect("input geometry poisoned") = InputGeometry {
                inner,
                style: text_style,
                layout: measured.clone(),
            };
            if let Some(l) = &measured {
                let selected = if password {
                    let r = m.selection();
                    m.text[..r.start].graphemes(true).count()
                        ..m.text[..r.end].graphemes(true).count()
                } else {
                    m.text[..m.selection().start].chars().count()
                        ..m.text[..m.selection().end].chars().count()
                };
                for area in if m.preedit.is_empty() {
                    l.selection_rects(selected)
                } else {
                    Vec::new()
                } {
                    content = content.child(UiElement::panel(
                        cx.auto_id(),
                        area,
                        VisualStyle::filled(tokens.colors.accent).alpha(80),
                    ));
                }
                if !m.preedit.is_empty() {
                    let start = if password {
                        m.text[..m.selection().start].graphemes(true).count()
                    } else {
                        m.text[..m.selection().start].chars().count()
                    };
                    let end = display_cursor(&m, password);
                    for area in l.selection_rects(start..end) {
                        content = content.child(UiElement::panel(
                            cx.auto_id(),
                            UiRect::new(area.left, area.bottom - 1.0, area.right, area.bottom),
                            VisualStyle::filled(tokens.colors.accent),
                        ));
                    }
                }
            }
            let value = if value.is_empty() {
                placeholder.clone()
            } else {
                value
            };
            let text_bounds = if multiline {
                bounds
            } else {
                UiRect::new(
                    bounds.left,
                    bounds.top,
                    bounds.left + 1_000_000.0,
                    bounds.bottom,
                )
            };
            content = content.child(
                UiElement::text(cx.auto_id(), text_bounds, value, text_style).paint_bounds(inner),
            );
            if m.focused {
                let index = display_cursor(&m, password);
                if let Some(caret) = measured.as_ref().and_then(|l| l.caret_rect(index)) {
                    root = root.ime_cursor_rect(caret);
                    content = content.child(UiElement::panel(
                        cx.auto_id(),
                        UiRect::new(caret.left, caret.top, caret.left + 1.0, caret.bottom),
                        VisualStyle::filled(tokens.colors.accent),
                    ));
                }
            }
            root = root
                .child(content)
                .on_action("input.set_value", move |_, action| {
                    if read_only {
                        return;
                    }
                    let before = semantic_model.get();
                    let mut next = before.clone();
                    next.select_all();
                    let value = action.payload_value().unwrap_or_default();
                    next.replace_selection(&if multiline {
                        value.replace("\r\n", "\n").replace('\r', "\n")
                    } else {
                        value.replace(['\n', '\r'], "")
                    });
                    commit(
                        &semantic_model,
                        before,
                        next,
                        &semantic_changed,
                        &semantic_selected,
                    );
                })
                .on_action("input.pointer_down", move |_, action| {
                    select_at(
                        &selection_model,
                        &mouse_geometry,
                        action,
                        rect,
                        false,
                        password,
                        &click_selected,
                    )
                })
                .on_action("input.pointer_drag", move |_, action| {
                    select_at(
                        &dragging,
                        &drag_geometry,
                        action,
                        rect,
                        true,
                        password,
                        &selected,
                    )
                });
            root
        })
        .on_input(move |cx, text| {
            if read_only {
                return;
            }
            cx.prevent_default();
            let before = model.get();
            let mut next = before.clone();
            next.replace_selection(&if multiline {
                text.replace("\r\n", "\n").replace('\r', "\n")
            } else {
                text.replace(['\n', '\r'], "")
            });
            keep_visible(
                &mut next,
                &edit_geometry.lock().expect("input geometry poisoned"),
                multiline,
                password,
            );
            commit(&model, before, next, &on_edit, &on_select);
        })
        .on_key_down(move |cx, key| {
            if key.state != KeyState::Down {
                return;
            }
            let before = key_model.get();
            if !before.preedit.is_empty() {
                return;
            }
            let mut next = before.clone();
            next.select(next.anchor, next.cursor);
            let extend = key.modifiers.shift();
            let ctrl = key.modifiers.ctrl() || key.modifiers.meta();
            let mut handled = true;
            match &key.key {
                LogicalKey::Named(NamedKey::ArrowLeft) if ctrl => next.move_word(false, extend),
                LogicalKey::Named(NamedKey::ArrowRight) if ctrl => next.move_word(true, extend),
                LogicalKey::Named(NamedKey::ArrowLeft) => next.move_cursor(false, extend),
                LogicalKey::Named(NamedKey::ArrowRight) => next.move_cursor(true, extend),
                LogicalKey::Named(NamedKey::ArrowUp) if multiline => move_visual(
                    &mut next,
                    &key_geometry.lock().expect("input geometry poisoned"),
                    false,
                    extend,
                    password,
                ),
                LogicalKey::Named(NamedKey::ArrowDown) if multiline => move_visual(
                    &mut next,
                    &key_geometry.lock().expect("input geometry poisoned"),
                    true,
                    extend,
                    password,
                ),
                LogicalKey::Named(NamedKey::Home) => {
                    next.cursor = if ctrl {
                        0
                    } else {
                        next.text[..next.cursor].rfind('\n').map_or(0, |i| i + 1)
                    };
                    if !extend {
                        next.anchor = next.cursor;
                    }
                }
                LogicalKey::Named(NamedKey::End) => {
                    next.cursor = if ctrl {
                        next.text.len()
                    } else {
                        next.text[next.cursor..]
                            .find('\n')
                            .map_or(next.text.len(), |i| next.cursor + i)
                    };
                    if !extend {
                        next.anchor = next.cursor;
                    }
                }
                LogicalKey::Named(NamedKey::Backspace) if !read_only => next.backspace(),
                LogicalKey::Named(NamedKey::Delete) if !read_only => next.delete_forward(),
                LogicalKey::Named(NamedKey::Enter) if multiline && !read_only => {
                    next.replace_selection("\n")
                }
                LogicalKey::Character(k) if ctrl && k.eq_ignore_ascii_case("a") => {
                    next.select_all()
                }
                LogicalKey::Character(k) if ctrl && k.eq_ignore_ascii_case("z") && !read_only => {
                    if extend {
                        next.redo()
                    } else {
                        next.undo()
                    }
                }
                LogicalKey::Character(k) if ctrl && k.eq_ignore_ascii_case("y") && !read_only => {
                    next.redo()
                }
                LogicalKey::Character(k)
                    if ctrl && (k.eq_ignore_ascii_case("c") || k.eq_ignore_ascii_case("x")) =>
                {
                    if !password {
                        if let Some(write) = &clipboard.write {
                            write(next.text[next.selection()].to_owned());
                        }
                        if k.eq_ignore_ascii_case("x") && !read_only {
                            next.replace_selection("");
                        }
                    }
                }
                LogicalKey::Character(k) if ctrl && k.eq_ignore_ascii_case("v") && !read_only => {
                    if let Some(read) = &clipboard.read {
                        let text = read();
                        next.replace_selection(&if multiline {
                            text
                        } else {
                            text.replace(['\n', '\r'], "")
                        });
                    }
                }
                _ => handled = false,
            }
            if handled {
                cx.prevent_default();
                keep_visible(
                    &mut next,
                    &key_geometry.lock().expect("input geometry poisoned"),
                    multiline,
                    password,
                );
                commit(&key_model, before, next, &key_changed, &key_selected);
            }
        })
        .on_wheel(move |cx, delta| {
            let geo = wheel_geometry.lock().expect("input geometry poisoned");
            let factor = if delta.unit == WheelUnit::Lines {
                48.0
            } else {
                1.0
            };
            scrolling.update(|m| {
                if let Some(l) = &geo.layout {
                    if multiline {
                        m.scroll.y = (m.scroll.y - delta.y * factor)
                            .clamp(0.0, (l.height - geo.inner.height()).max(0.0));
                    } else {
                        m.scroll.x = (m.scroll.x - (delta.x + delta.y) * factor)
                            .clamp(0.0, (l.width - geo.inner.width()).max(0.0));
                    }
                }
            });
            cx.prevent_default();
        })
        .on_composition_update(move |_, text, cursor| {
            if !read_only {
                composing.update(|m| {
                    m.preedit = text.to_owned();
                    m.preedit_cursor = cursor;
                    keep_visible(
                        m,
                        &composition_geometry
                            .lock()
                            .expect("input geometry poisoned"),
                        multiline,
                        password,
                    );
                });
            }
        })
        .on_composition_end({
            let state = focus_model.clone();
            move |_| {
                state.update(|m| {
                    m.preedit.clear();
                    m.preedit_cursor = None;
                })
            }
        })
        .on_focus(move |_| {
            focus_model.update(|m| {
                m.focused = true;
                keep_visible(
                    m,
                    &focus_geometry.lock().expect("input geometry poisoned"),
                    multiline,
                    password,
                );
            })
        })
        .on_blur(move |_| {
            blur_model.update(|m| {
                m.focused = false;
                m.preedit.clear();
            })
        });
        if let Some(name) = public_id {
            element.public_id(name)
        } else {
            element
        }
    }
}
impl IntoElementContent for TextInput {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self.into());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_respects_graphemes_selection_and_undo() {
        let mut m = TextInputModel::new("a👩‍💻中");
        m.backspace();
        assert_eq!(m.text, "a👩‍💻");
        m.backspace();
        assert_eq!(m.text, "a");
        m.undo();
        assert_eq!(m.text, "a👩‍💻");
        m.select_all();
        m.replace_selection("β");
        assert_eq!(m.cursor, 2);
        m.undo();
        assert_eq!(m.text, "a👩‍💻");
        m.redo();
        assert_eq!(m.text, "β");
    }
    #[test]
    fn vertical_selection_and_invalid_offsets_stay_on_boundaries() {
        let mut m = TextInputModel::new("αβ\n中xy");
        m.select(1, 4);
        assert!(m.text.is_char_boundary(m.anchor));
        m.select(4, 4);
        m.move_vertical(true, false);
        assert_eq!(&m.text[..m.cursor], "αβ\n中x");
        m.move_cursor(false, true);
        assert!(!m.selection().is_empty());
    }
}
