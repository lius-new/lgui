use super::scrollbar;
use crate::WidgetValue;
use crate::core::*;
use crate::theme::ThemeContext;
use std::{collections::BTreeSet, ops::Range, sync::Arc};
#[derive(Clone, Debug, PartialEq)]
pub enum RowHeights {
    Uniform {
        count: usize,
        height: f32,
    },
    Variable {
        heights: Arc<[f32]>,
        prefix: Arc<[f32]>,
    },
}
fn valid(v: f32) -> f32 {
    if v.is_finite() { v.max(1.0) } else { 1.0 }
}
impl RowHeights {
    pub fn uniform(count: usize, height: f32) -> Self {
        Self::Uniform {
            count,
            height: valid(height),
        }
    }
    pub fn variable(heights: impl IntoIterator<Item = f32>) -> Self {
        let heights: Vec<_> = heights.into_iter().map(valid).collect();
        let mut prefix = Vec::with_capacity(heights.len() + 1);
        prefix.push(0.0);
        for h in &heights {
            prefix.push(prefix.last().unwrap() + h);
        }
        Self::Variable {
            heights: heights.into(),
            prefix: prefix.into(),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Uniform { count, .. } => *count,
            Self::Variable { heights, .. } => heights.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn top(&self, index: usize) -> f32 {
        let i = index.min(self.len());
        match self {
            Self::Uniform { height, .. } => i as f32 * valid(*height),
            Self::Variable { prefix, .. } => prefix[i],
        }
    }
    pub fn height(&self, index: usize) -> f32 {
        if index >= self.len() {
            0.0
        } else {
            self.top(index + 1) - self.top(index)
        }
    }
    pub fn total(&self) -> f32 {
        self.top(self.len())
    }
    pub fn visible_range(&self, offset: f32, viewport: f32, overscan: usize) -> Range<usize> {
        let offset = if offset.is_finite() {
            offset.max(0.0)
        } else {
            0.0
        };
        let viewport = if viewport.is_finite() {
            viewport.max(0.0)
        } else {
            0.0
        };
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let m = lo + (hi - lo) / 2;
            if self.top(m + 1) <= offset {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        let start = lo;
        let (mut lo, mut hi) = (start, self.len());
        while lo < hi {
            let m = lo + (hi - lo) / 2;
            if self.top(m) < offset + viewport {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        start.saturating_sub(overscan)..lo.saturating_add(overscan).min(self.len())
    }
}
pub fn virtual_list(
    rect: UiRect,
    heights: Arc<RowHeights>,
    offset: impl Into<WidgetValue<f32>>,
    render_row: impl Fn(usize, UiRect) -> Element,
) -> Element {
    let offset = offset.into();
    let total = heights.total();
    let max = (total - rect.height()).max(0.0);
    let current = if offset.get().is_finite() {
        offset.get().clamp(0.0, max)
    } else {
        0.0
    };
    let range = heights.visible_range(current, rect.height(), 1);
    let show_bar = max > 0.0;
    let right = rect.right - if show_bar { 8.0 } else { 0.0 };
    let mut children = Vec::new();
    for i in range {
        let top = rect.top + heights.top(i) - current;
        children.push(
            render_row(
                i,
                UiRect::new(rect.left, top, right, top + heights.height(i)),
            )
            .key_if_absent(i.to_string()),
        );
    }
    if show_bar {
        children.push(
            scrollbar(
                UiRect::new(rect.right - 6.0, rect.top, rect.right, rect.bottom),
                Axis::Vertical,
                rect.height(),
                total,
                offset.clone(),
            )
            .key("scrollbar"),
        );
    }
    let wheel = offset.clone();
    let keys = offset;
    Element::new(move |cx| {
        UiElement::clip(cx.id, rect, 0.0, 0.0)
            .interaction(InteractionRole::Custom("virtual-list"))
            .semantics(Semantics::new(SemanticRole::List))
            .children(cx.children)
    })
    .children(children)
    .on_wheel(move |cx, delta| {
        cx.prevent_default();
        let factor = if delta.unit == WheelUnit::Lines {
            48.0
        } else {
            1.0
        };
        wheel.set((wheel.get() - delta.y * factor).clamp(0.0, max));
    })
    .on_key_down(move |cx, key| {
        let value = match key.key {
            LogicalKey::Named(NamedKey::PageDown) => Some(keys.get() + rect.height()),
            LogicalKey::Named(NamedKey::PageUp) => Some(keys.get() - rect.height()),
            LogicalKey::Named(NamedKey::Home) => Some(0.0),
            LogicalKey::Named(NamedKey::End) => Some(max),
            _ => None,
        };
        if let Some(value) = value {
            cx.prevent_default();
            keys.set(value.clamp(0.0, max));
        }
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub id: Arc<str>,
    pub label: Arc<str>,
    pub children: Vec<TreeNode>,
}
impl TreeNode {
    pub fn new(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
        }
    }
    pub fn child(mut self, node: Self) -> Self {
        self.children.push(node);
        self
    }
}
#[derive(Clone, Debug)]
struct TreeRow {
    id: Arc<str>,
    label: Arc<str>,
    depth: usize,
    parent: Option<Arc<str>>,
    has_children: bool,
    expanded: bool,
}
fn flatten(
    nodes: &[TreeNode],
    expanded: &BTreeSet<Arc<str>>,
    depth: usize,
    parent: Option<&Arc<str>>,
    out: &mut Vec<TreeRow>,
) {
    for n in nodes {
        let open = expanded.contains(&n.id);
        out.push(TreeRow {
            id: n.id.clone(),
            label: n.label.clone(),
            depth,
            parent: parent.cloned(),
            has_children: !n.children.is_empty(),
            expanded: open,
        });
        if open {
            flatten(&n.children, expanded, depth + 1, Some(&n.id), out);
        }
    }
}
pub fn tree_view(
    rect: UiRect,
    nodes: Arc<[TreeNode]>,
    expanded: impl Into<WidgetValue<BTreeSet<Arc<str>>>>,
    selected: impl Into<WidgetValue<Option<Arc<str>>>>,
    offset: impl Into<WidgetValue<f32>>,
) -> Element {
    let expanded = expanded.into();
    let selected = selected.into();
    let offset = offset.into();
    let mut rows = Vec::new();
    flatten(&nodes, &expanded.get(), 0, None, &mut rows);
    let rows = Arc::new(rows);
    let keys = rows.clone();
    let exp = expanded.clone();
    let sel = selected.clone();
    let scroll = offset.clone();
    virtual_list(
        rect,
        Arc::new(RowHeights::uniform(rows.len(), 26.0)),
        offset,
        move |i, row| {
            let item = rows[i].clone();
            let is_selected = selected.get().as_ref() == Some(&item.id);
            let selected = selected.clone();
            let expand = expanded.clone();
            let id = item.id.clone();
            let stable_key = id.clone();
            let label = format!(
                "{}{}",
                if item.has_children {
                    if item.expanded { "▾ " } else { "▸ " }
                } else {
                    "  "
                },
                item.label
            );
            let mut sem = Semantics::new(SemanticRole::TreeItem).name(item.label.to_string());
            sem.state.selected = is_selected;
            sem.state.expanded = item.has_children.then_some(item.expanded);
            Element::new(move |cx| {
                let t = cx
                    .try_use_context::<ThemeContext>()
                    .map(|t| *t.tokens())
                    .unwrap_or_default();
                let mut root = UiElement::panel(
                    cx.id.clone(),
                    row,
                    VisualStyle::filled(if is_selected {
                        t.colors.surface_hover
                    } else {
                        t.colors.surface
                    }),
                )
                .interaction(InteractionRole::Row)
                .semantics(sem);
                let text = UiRect::new(
                    row.left + item.depth as f32 * 16.0 + 4.0,
                    row.top,
                    row.right,
                    row.bottom,
                );
                root = root.child(
                    UiElement::text(
                        cx.auto_id(),
                        text,
                        label,
                        TextStyle::new(t.colors.text, t.typography.body_size.abs() as f32, 400),
                    )
                    .overflow(Overflow::Ellipsis),
                );
                root
            })
            .key(stable_key.as_ref())
            .on_click_event(move |cx| {
                selected.set(Some(id.clone()));
                if item.has_children {
                    expand.update(|set| {
                        if !set.remove(&id) {
                            set.insert(id.clone());
                        }
                    });
                }
                cx.stop_propagation();
            })
        },
    )
    .semantics(Semantics::new(SemanticRole::Tree))
    .on_key_down(move |cx, key| {
        if keys.is_empty() {
            return;
        }
        let current = sel.get();
        let i = keys
            .iter()
            .position(|r| Some(&r.id) == current.as_ref())
            .unwrap_or(0);
        let mut next = i;
        let mut handled = true;
        match key.key {
            LogicalKey::Named(NamedKey::ArrowDown) => next = (i + 1).min(keys.len() - 1),
            LogicalKey::Named(NamedKey::ArrowUp) => next = i.saturating_sub(1),
            LogicalKey::Named(NamedKey::Home) => next = 0,
            LogicalKey::Named(NamedKey::End) => next = keys.len() - 1,
            LogicalKey::Named(NamedKey::ArrowRight) => {
                if keys[i].has_children {
                    exp.update(|set| {
                        set.insert(keys[i].id.clone());
                    });
                }
            }
            LogicalKey::Named(NamedKey::ArrowLeft) => {
                if keys[i].expanded {
                    exp.update(|set| {
                        set.remove(&keys[i].id);
                    });
                } else if let Some(parent) = &keys[i].parent
                    && let Some(p) = keys.iter().position(|r| &r.id == parent)
                {
                    next = p;
                }
            }
            _ => handled = false,
        }
        if handled {
            cx.prevent_default();
            sel.set(Some(keys[next].id.clone()));
            let top = next as f32 * 26.0;
            if top < scroll.get() {
                scroll.set(top);
            } else if top + 26.0 > scroll.get() + rect.height() {
                scroll.set((top + 26.0 - rect.height()).max(0.0));
            }
        }
    })
}
pub fn tabs(
    rect: UiRect,
    labels: Arc<[String]>,
    selected: impl Into<WidgetValue<usize>>,
    render_page: impl FnOnce(usize, UiRect) -> Element,
) -> Element {
    let selected = selected.into();
    let header = 32.0_f32.min(rect.height().max(0.0));
    let body = UiRect::new(rect.left, rect.top + header, rect.right, rect.bottom);
    let index = selected.get().min(labels.len().saturating_sub(1));
    let mut children = Vec::new();
    let width = if labels.is_empty() {
        0.0
    } else {
        rect.width() / labels.len() as f32
    };
    for (i, label) in labels.iter().enumerate() {
        let area = UiRect::new(
            rect.left + width * i as f32,
            rect.top,
            rect.left + width * (i + 1) as f32,
            rect.top + header,
        );
        let label = label.clone();
        let state = selected.clone();
        children.push(
            Element::new(move |cx| {
                let t = cx
                    .try_use_context::<ThemeContext>()
                    .map(|t| *t.tokens())
                    .unwrap_or_default();
                let mut sem = Semantics::new(SemanticRole::Tab)
                    .name(label.clone())
                    .action(SemanticAction::Click);
                sem.state.selected = i == index;
                UiElement::button(
                    cx.id.clone(),
                    area,
                    VisualStyle::filled(if i == index {
                        t.colors.surface_hover
                    } else {
                        t.colors.surface
                    }),
                )
                .interaction(InteractionRole::Button)
                .semantics(sem)
                .child(
                    UiElement::text(
                        cx.auto_id(),
                        area,
                        label,
                        TextStyle::new(t.colors.text, t.typography.body_size.abs() as f32, 400)
                            .centered(),
                    )
                    .overflow(Overflow::Ellipsis),
                )
            })
            .key(i.to_string())
            .on_click(move || state.set(i)),
        );
    }
    let count = labels.len();
    let headers = group(UiRect::new(
        rect.left,
        rect.top,
        rect.right,
        rect.top + header,
    ))
    .semantics(Semantics::new(SemanticRole::TabList))
    .children(children)
    .on_key_down(move |cx, key| {
        if count == 0 {
            return;
        }
        let current = selected.get().min(count - 1);
        let next = match key.key {
            LogicalKey::Named(NamedKey::ArrowRight) => Some((current + 1) % count),
            LogicalKey::Named(NamedKey::ArrowLeft) => Some((current + count - 1) % count),
            LogicalKey::Named(NamedKey::Home) => Some(0),
            LogicalKey::Named(NamedKey::End) => Some(count - 1),
            _ => None,
        };
        if let Some(next) = next {
            cx.prevent_default();
            selected.set(next);
        }
    });
    let mut root = group(rect).child(headers);
    if count > 0 {
        root = root.child(
            group(body)
                .semantics(Semantics::new(SemanticRole::TabPanel))
                .child(render_page(index, body)),
        );
    }
    root
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_height_virtualization_handles_boundaries_and_overscan() {
        let h = RowHeights::variable([10.0, 30.0, 20.0, 40.0]);
        assert_eq!(h.total(), 100.0);
        assert_eq!(h.visible_range(10.0, 30.0, 0), 1..2);
        assert_eq!(h.visible_range(39.0, 2.0, 0), 1..3);
        assert_eq!(h.visible_range(39.0, 2.0, 1), 0..4);
        assert_eq!(
            RowHeights::uniform(1_000_000, 20.0).visible_range(10000.0, 200.0, 1),
            499..511
        );
    }
    #[test]
    fn collapsed_tree_branches_are_not_flattened() {
        let nodes = [
            TreeNode::new("a", "A").child(TreeNode::new("b", "B")),
            TreeNode::new("c", "C"),
        ];
        let mut rows = Vec::new();
        flatten(&nodes, &BTreeSet::new(), 0, None, &mut rows);
        assert_eq!(rows.len(), 2);
        rows.clear();
        flatten(
            &nodes,
            &[Arc::<str>::from("a")].into_iter().collect(),
            0,
            None,
            &mut rows,
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].parent.as_deref(), Some("a"));
    }
}
