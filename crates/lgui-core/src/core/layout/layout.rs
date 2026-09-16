use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
};

use super::{EdgeInsets, HostTree, Size, UiId, UiRect};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutSpec {
    Absolute,
    Stack {
        axis: Axis,
        gap: f32,
        padding: EdgeInsets,
        align: Align,
    },
    Fixed(Size),
}

impl Eq for LayoutSpec {}

impl Hash for LayoutSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Absolute => {}
            Self::Stack {
                axis,
                gap,
                padding,
                align,
            } => {
                axis.hash(state);
                gap.to_bits().hash(state);
                padding.hash(state);
                align.hash(state);
            }
            Self::Fixed(size) => size.hash(state),
        }
    }
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self::Absolute
    }
}

pub fn apply_layout(tree: &mut HostTree, root: UiId) {
    let Some(root_node) = tree.node(&root).cloned() else {
        return;
    };
    let LayoutSpec::Stack {
        axis,
        gap,
        padding,
        align,
    } = root_node.layout
    else {
        return;
    };
    let children = root_node.children.clone();
    let mut cursor = match axis {
        Axis::Horizontal => root_node.layout_rect.left + padding.left,
        Axis::Vertical => root_node.layout_rect.top + padding.top,
    };
    let content = root_node.layout_rect.inset(padding);
    for child_id in children {
        let Some(child) = tree.node_mut(&child_id) else {
            continue;
        };
        let size = Size::new(child.layout_rect.width(), child.layout_rect.height());
        let rect = stack_child_rect(content, axis, align, cursor, size);
        child.layout_rect = rect;
        child.hit_rect = rect;
        child.paint_bounds = rect;
        cursor += match axis {
            Axis::Horizontal => size.width + gap,
            Axis::Vertical => size.height + gap,
        };
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutCommitMetrics {
    pub visited_nodes: usize,
    pub laid_out_nodes: usize,
    pub reused_nodes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutInvalidation {
    None,
    SelfOnly,
    Subtree,
    BubbleToLayoutBoundary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LayoutInput {
    parent: Option<UiId>,
    children: Vec<UiId>,
    spec: LayoutSpec,
    rect: UiRect,
    hit_rect: UiRect,
    paint_bounds: UiRect,
}

#[derive(Clone)]
struct LayoutOutput {
    layout_rect: UiRect,
    hit_rect: UiRect,
    paint_bounds: UiRect,
}

#[derive(Default)]
pub struct LayoutRuntime {
    inputs: HashMap<UiId, LayoutInput>,
    outputs: HashMap<UiId, LayoutOutput>,
    metrics: LayoutCommitMetrics,
}

impl LayoutRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves logical layout while retaining committed results per node. Paint-only changes do
    /// not execute layout; geometric changes execute only their nearest layout boundary subtree.
    pub fn update(&mut self, tree: &mut HostTree) -> LayoutCommitMetrics {
        let changes = tree.take_projection_changes();
        self.update_projection(tree, changes).0
    }

    pub(crate) fn update_projection(
        &mut self,
        tree: &mut HostTree,
        mut changes: super::ProjectionChanges,
    ) -> (LayoutCommitMetrics, super::ProjectionChanges) {
        let first_layout = self.inputs.is_empty();
        let candidate_ids = if first_layout {
            tree.nodes()
                .iter()
                .map(|node| node.id.clone())
                .collect::<HashSet<_>>()
        } else {
            changes.changed.clone()
        };
        let next_inputs = candidate_ids
            .iter()
            .filter_map(|id| {
                let node = tree.node(id)?;
                Some((
                    id.clone(),
                    LayoutInput {
                        parent: node.parent.clone(),
                        children: node.children.clone(),
                        spec: node.layout,
                        rect: node.layout_rect,
                        hit_rect: node.hit_rect,
                        paint_bounds: node.paint_bounds,
                    },
                ))
            })
            .collect::<HashMap<_, _>>();
        let visited_nodes = next_inputs.len();
        let mut dirty = HashSet::new();

        if first_layout {
            dirty.extend(
                tree.nodes()
                    .iter()
                    .filter(|node| node.parent.is_none())
                    .map(|node| node.id.clone()),
            );
        } else {
            for (id, input) in &next_inputs {
                match self.inputs.get(id) {
                    None => {
                        dirty.insert(layout_boundary(tree, id));
                    }
                    Some(previous) if previous != input => {
                        dirty.insert(layout_boundary(tree, id));
                        if previous.parent != input.parent || previous.children != input.children {
                            if let Some(parent) = previous
                                .parent
                                .as_ref()
                                .filter(|parent| next_inputs.contains_key(*parent))
                            {
                                dirty.insert(layout_boundary(tree, parent));
                            }
                        }
                    }
                    Some(_) => {}
                }
            }
            for removed in &changes.removed {
                if let Some(parent) = self.inputs.get(removed).and_then(|input| {
                    input
                        .parent
                        .as_ref()
                        .filter(|parent| tree.node(parent).is_some())
                }) {
                    dirty.insert(layout_boundary(tree, parent));
                }
            }
        }

        for id in &candidate_ids {
            if let (Some(node), Some(output)) = (tree.node_mut(id), self.outputs.get(id)) {
                node.layout_rect = output.layout_rect;
                node.hit_rect = output.hit_rect;
                node.paint_bounds = output.paint_bounds;
            }
        }

        let dirty_roots = minimal_dirty_roots(tree, dirty);
        let laid_out_ids = dirty_roots
            .iter()
            .flat_map(|root| subtree_ids(tree, root))
            .collect::<HashSet<_>>();
        for id in &laid_out_ids {
            let Some(input) = next_inputs.get(id) else {
                continue;
            };
            if let Some(node) = tree.node_mut(id) {
                node.layout_rect = input.rect;
                node.hit_rect = input.hit_rect;
                node.paint_bounds = input.paint_bounds;
            }
        }
        for root in dirty_roots {
            layout_subtree(tree, root);
        }
        for removed in &changes.removed {
            self.inputs.remove(&removed);
            self.outputs.remove(&removed);
        }
        self.inputs.extend(next_inputs);
        for id in &laid_out_ids {
            let Some(node) = tree.node(id) else {
                continue;
            };
            self.outputs.insert(
                id.clone(),
                LayoutOutput {
                    layout_rect: node.layout_rect,
                    hit_rect: node.hit_rect,
                    paint_bounds: node.paint_bounds,
                },
            );
        }
        let laid_out_nodes = laid_out_ids.len();
        changes.changed.extend(laid_out_ids);
        self.metrics = LayoutCommitMetrics {
            visited_nodes,
            laid_out_nodes,
            reused_nodes: visited_nodes.saturating_sub(laid_out_nodes),
        };
        (self.metrics, changes)
    }

    pub fn metrics(&self) -> LayoutCommitMetrics {
        self.metrics
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn layout_boundary(tree: &HostTree, id: &UiId) -> UiId {
    let Some(node) = tree.node(id) else {
        return id.clone();
    };
    let Some(parent_id) = node.parent.as_ref() else {
        return id.clone();
    };
    if tree
        .node(parent_id)
        .is_some_and(|parent| matches!(parent.layout, LayoutSpec::Stack { .. }))
    {
        parent_id.clone()
    } else {
        id.clone()
    }
}

fn minimal_dirty_roots(tree: &HostTree, dirty: HashSet<UiId>) -> Vec<UiId> {
    dirty
        .iter()
        .filter(|candidate| {
            let mut parent = tree.node(candidate).and_then(|node| node.parent.as_ref());
            while let Some(parent_id) = parent {
                if dirty.contains(parent_id) {
                    return false;
                }
                parent = tree.node(parent_id).and_then(|node| node.parent.as_ref());
            }
            true
        })
        .cloned()
        .collect()
}

fn subtree_ids(tree: &HostTree, root: &UiId) -> Vec<UiId> {
    let Some(node) = tree.node(root) else {
        return Vec::new();
    };
    let mut ids = vec![root.clone()];
    for child in &node.children {
        ids.extend(subtree_ids(tree, child));
    }
    ids
}

pub fn apply_layout_tree(tree: &mut HostTree) {
    let roots = tree
        .nodes()
        .iter()
        .filter(|node| node.parent.is_none())
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    for root in roots {
        layout_subtree(tree, root);
    }
}

fn layout_subtree(tree: &mut HostTree, root: UiId) {
    let Some(root_node) = tree.node(&root).cloned() else {
        return;
    };
    if let LayoutSpec::Stack {
        axis,
        gap,
        padding,
        align,
    } = root_node.layout
    {
        let mut cursor = match axis {
            Axis::Horizontal => root_node.layout_rect.left + padding.left,
            Axis::Vertical => root_node.layout_rect.top + padding.top,
        };
        let content = root_node.layout_rect.inset(padding);
        for child_id in &root_node.children {
            let Some(child) = tree.node(child_id).cloned() else {
                continue;
            };
            let size = match child.layout {
                LayoutSpec::Fixed(size) => size,
                _ => Size::new(child.layout_rect.width(), child.layout_rect.height()),
            };
            let rect = stack_child_rect(content, axis, align, cursor, size);
            translate_subtree(
                tree,
                child_id,
                rect.left - child.layout_rect.left,
                rect.top - child.layout_rect.top,
            );
            cursor += match axis {
                Axis::Horizontal => size.width + gap,
                Axis::Vertical => size.height + gap,
            };
        }
    }
    for child in root_node.children {
        layout_subtree(tree, child);
    }
}

fn translate_subtree(tree: &mut HostTree, root: &UiId, x: f32, y: f32) {
    if x == 0.0 && y == 0.0 {
        return;
    }
    let Some(node) = tree.node(root).cloned() else {
        return;
    };
    let children = node.children.clone();
    if let Some(current) = tree.node_mut(root) {
        *current = node.translate(x, y);
    }
    for child in children {
        translate_subtree(tree, &child, x, y);
    }
}

fn stack_child_rect(content: UiRect, axis: Axis, align: Align, cursor: f32, size: Size) -> UiRect {
    match axis {
        Axis::Horizontal => {
            let top = match align {
                Align::Start => content.top,
                Align::Center => content.top + (content.height() - size.height) / 2.0,
                Align::End => content.bottom - size.height,
                Align::Stretch => content.top,
            };
            let bottom = if align == Align::Stretch {
                content.bottom
            } else {
                top + size.height
            };
            UiRect::new(cursor, top, cursor + size.width, bottom)
        }
        Axis::Vertical => {
            let left = match align {
                Align::Start => content.left,
                Align::Center => content.left + (content.width() - size.width) / 2.0,
                Align::End => content.right - size.width,
                Align::Stretch => content.left,
            };
            let right = if align == Align::Stretch {
                content.right
            } else {
                left + size.width
            };
            UiRect::new(left, cursor, right, cursor + size.height)
        }
    }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod tests;
