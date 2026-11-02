use std::collections::{HashMap, HashSet};

use super::SizeConstraints;
use super::measure::{
    IntrinsicInput, MeasurementCache, content_rect, desired_size, intrinsic_input, rect_size,
};
#[cfg(test)]
use super::{EdgeInsets, Size};
use super::{HostTree, LayoutSpec, UiId, UiRect};

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

pub fn apply_layout(tree: &mut HostTree, root: UiId) {
    layout_subtree(tree, root);
    finish_layout(tree);
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
    sizing: Option<SizeConstraints>,
    item: super::LayoutItem,
    position: super::Position,
    visibility: super::Visibility,
    overflow: super::Overflow,
    public_id: Option<std::sync::Arc<str>>,
    z_index: i32,
    clip: Option<UiRect>,
    offset: crate::core::Point,
    intrinsic: Option<IntrinsicInput>,
    rect: UiRect,
    hit_rect: UiRect,
    paint_bounds: UiRect,
}

#[derive(Clone)]
struct LayoutOutput {
    layout_rect: UiRect,
    hit_rect: UiRect,
    paint_bounds: UiRect,
    clip: Option<UiRect>,
    origin: Option<UiRect>,
    display: Option<std::borrow::Cow<'static, str>>,
    overflow_hidden: bool,
}

#[derive(Default)]
pub struct LayoutRuntime {
    inputs: HashMap<UiId, LayoutInput>,
    outputs: HashMap<UiId, LayoutOutput>,
    metrics: LayoutCommitMetrics,
    viewport: Option<UiRect>,
    force_layout: bool,
    position_errors: Vec<super::PositionError>,
}

impl LayoutRuntime {
    pub fn set_viewport(&mut self, viewport: UiRect) {
        if self.viewport != Some(viewport) {
            self.viewport = Some(viewport);
            self.force_layout = true;
        }
    }
    pub fn position_errors(&self) -> &[super::PositionError] {
        &self.position_errors
    }
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
        let first_layout = self.inputs.is_empty() || self.force_layout;
        self.force_layout = false;
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
                        spec: node.layout.clone(),
                        sizing: node.size_constraints,
                        item: node.layout_item,
                        position: node.position.clone(),
                        visibility: node.visibility,
                        overflow: node.overflow,
                        public_id: node.public_id.clone(),
                        z_index: node.z_index,
                        clip: node.clip_rect,
                        offset: crate::core::Point::new(
                            node.content_offset.0,
                            node.content_offset.1,
                        ),
                        intrinsic: intrinsic_input(node),
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
                        if (previous.parent != input.parent || previous.children != input.children)
                            && let Some(parent) = previous
                                .parent
                                .as_ref()
                                .filter(|parent| next_inputs.contains_key(*parent))
                        {
                            dirty.insert(layout_boundary(tree, parent));
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
                node.clip_rect = output.clip;
                node.position_origin = output.origin;
                node.display_text = output.display.clone();
                node.overflow_hidden = output.overflow_hidden;
            }
        }

        let dirty_roots = minimal_dirty_roots(tree, dirty);
        let mut laid_out_ids = dirty_roots
            .iter()
            .flat_map(|root| subtree_ids(tree, root))
            .collect::<HashSet<_>>();
        for id in &laid_out_ids {
            let Some(input) = next_inputs.get(id).or_else(|| self.inputs.get(id)) else {
                continue;
            };
            if let Some(node) = tree.node_mut(id) {
                node.layout_rect = input.rect;
                node.hit_rect = input.hit_rect;
                node.paint_bounds = input.paint_bounds;
                node.clip_rect = input.clip;
                node.position_origin = None;
                node.display_text = None;
                node.overflow_hidden = false;
            }
        }
        for root in dirty_roots {
            layout_subtree(tree, root);
        }
        if !laid_out_ids.is_empty() {
            let viewport = self.viewport.unwrap_or_else(|| {
                tree.nodes()
                    .iter()
                    .filter(|n| n.parent.is_none())
                    .map(|n| n.layout_rect)
                    .reduce(UiRect::union)
                    .unwrap_or_default()
            });
            let (positioned, errors) = super::position::apply(tree, viewport);
            self.position_errors = errors;
            laid_out_ids.extend(positioned);
            super::overflow::apply(tree, &laid_out_ids);
            tree.reorder_by_hierarchy();
            let extra = tree.take_projection_changes();
            changes.structure_changed |= extra.structure_changed;
            changes.changed.extend(extra.changed);
        }
        for removed in &changes.removed {
            self.inputs.remove(removed);
            self.outputs.remove(removed);
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
                    clip: node.clip_rect,
                    origin: node.position_origin,
                    display: node.display_text.clone(),
                    overflow_hidden: node.overflow_hidden,
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
    let mut boundary = id.clone();
    let mut parent = tree.node(id).and_then(|node| node.parent.as_ref());
    while let Some(parent_id) = parent {
        let Some(node) = tree.node(parent_id) else {
            break;
        };
        if node.layout.is_container()
            || node
                .size_constraints
                .is_some_and(|sizing| sizing.measures_content())
        {
            boundary = parent_id.clone();
        }
        parent = node.parent.as_ref();
    }
    boundary
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
    finish_layout(tree);
}

fn finish_layout(tree: &mut HostTree) {
    let viewport = tree
        .nodes()
        .iter()
        .filter(|n| n.parent.is_none())
        .map(|n| n.layout_rect)
        .reduce(UiRect::union)
        .unwrap_or_default();
    super::position::apply(tree, viewport);
    let ids = tree.nodes().iter().map(|n| n.id.clone()).collect();
    super::overflow::apply(tree, &ids);
    tree.reorder_by_hierarchy();
}

fn layout_subtree(tree: &mut HostTree, root: UiId) {
    arrange_subtree(tree, root, false, &mut MeasurementCache::new());
}

fn arrange_subtree(
    tree: &mut HostTree,
    root: UiId,
    sized_by_parent: bool,
    measurements: &mut MeasurementCache,
) {
    let Some(node) = tree.node(&root) else { return };
    let available = node
        .parent
        .as_ref()
        .and_then(|p| tree.node(p))
        .map(content_rect)
        .map(rect_size)
        .unwrap_or_else(|| rect_size(node.layout_rect));
    let collapsed = node.visibility == super::Visibility::Collapsed
        || super::measure::collapses(node, available);
    if let Some(n) = tree.node_mut(&root) {
        n.overflow_hidden = collapsed;
    }
    if collapsed {
        return;
    }
    let node = tree.node(&root).unwrap();
    if !sized_by_parent && node.size_constraints.is_some() {
        let rect = node.layout_rect;
        let available = node
            .parent
            .as_ref()
            .and_then(|id| tree.node(id))
            .map(content_rect)
            .unwrap_or(rect);
        let size = desired_size(tree, &root, rect_size(available), measurements);
        place_subtree(
            tree,
            &root,
            UiRect::new(
                rect.left,
                rect.top,
                rect.left + size.width,
                rect.top + size.height,
            ),
        );
    }
    let Some(root_node) = tree.node(&root).cloned() else {
        return;
    };
    for (child, rect) in super::containers::arrange(tree, &root_node, measurements) {
        place_subtree(tree, &child, rect);
    }
    for child in root_node.children {
        let sized = root_node.layout.is_container()
            && tree.node(&child).is_some_and(|n| n.position.participates());
        arrange_subtree(tree, child, sized, measurements);
    }
}

pub(super) fn place_subtree(tree: &mut HostTree, root: &UiId, rect: UiRect) {
    let Some(node) = tree.node(root) else { return };
    let old = node.layout_rect;
    translate_subtree(tree, root, rect.left - old.left, rect.top - old.top);
    if let Some(node) = tree.node_mut(root) {
        let dw = rect.width() - old.width();
        let dh = rect.height() - old.height();
        if node.clip_rect == Some(node.layout_rect) {
            node.clip_rect = Some(rect);
        }
        node.layout_rect = rect;
        node.hit_rect.right += dw;
        node.hit_rect.bottom += dh;
        node.paint_bounds.right += dw;
        node.paint_bounds.bottom += dh;
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

#[cfg(test)]
#[path = "layout_test.rs"]
mod tests;

#[cfg(test)]
#[path = "sizing_runtime_test.rs"]
mod sizing_runtime_tests;
