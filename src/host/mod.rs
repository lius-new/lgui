use std::collections::{HashMap, HashSet};
use std::fmt;
#[cfg(feature = "diagnostics-timing")]
use std::time::Instant;

use crate::{
    core::{
        compile_scene_root, patch_compositing_layer_spec, scene_root_ids, CompositingLayerSpec,
        HostTree, InteractionFlags, Scene, ScenePrimitive, UiId, UiInteractionState, UiNode,
        UiNodeKind, UiRect,
    },
    frame::{DirtyRegionSet, InvalidationRequest, InvalidationSet},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostNodeId {
    index: u32,
    generation: u32,
}

impl fmt::Display for HostNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.index, self.generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostUpdateKind {
    Layout,
    Paint,
    Interaction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostMutation {
    InsertNode {
        id: HostNodeId,
        source: UiId,
        bounds: UiRect,
    },
    RemoveNode {
        id: HostNodeId,
        source: UiId,
        old_bounds: UiRect,
    },
    UpdateProps {
        id: HostNodeId,
        source: UiId,
        kind: HostUpdateKind,
        old_bounds: UiRect,
        new_bounds: UiRect,
    },
    ReorderChildren {
        id: HostNodeId,
        source: UiId,
        bounds: UiRect,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneMutation {
    Insert(HostNodeId),
    Update(HostNodeId),
    Remove(HostNodeId),
    Reorder,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCommitMetrics {
    pub host_nodes: usize,
    pub scene_nodes: usize,
    pub visited_host_nodes: usize,
    pub compiled_scene_nodes: usize,
    pub host_mutations: usize,
    pub scene_mutations: usize,
    pub reused_scene_nodes: usize,
}

#[cfg(feature = "diagnostics-timing")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HostCommitTimings {
    pub change_scan_ms: f32,
    pub node_patch_ms: f32,
    pub scene_reconcile_ms: f32,
    pub scene_snapshot_ms: f32,
    pub damage_ms: f32,
    pub finalize_ms: f32,
}

#[derive(Clone, Debug)]
pub struct HostCommit {
    pub mutations: Vec<HostMutation>,
    pub scene_mutations: Vec<SceneMutation>,
    pub damage: DamageReport,
    pub scene: Scene,
    pub metrics: HostCommitMetrics,
    #[cfg(feature = "diagnostics-timing")]
    pub(crate) timings: HostCommitTimings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageReason {
    FirstCommit,
    Explicit,
    Insert,
    Remove,
    Layout,
    Paint,
    Interaction,
    Structure,
    Clean,
}

#[derive(Clone, Debug)]
pub struct DamageDetail {
    pub reason: DamageReason,
    pub node_id: Option<UiId>,
    pub old_bounds: Option<UiRect>,
    pub new_bounds: Option<UiRect>,
    pub rects: Vec<UiRect>,
}

#[derive(Clone, Debug)]
pub struct DamageReport {
    pub dirty: DirtyRegionSet,
    pub reasons: Vec<DamageReason>,
    pub details: Vec<DamageDetail>,
}

struct HostNode {
    source: UiId,
    mounted: bool,
    parent: Option<HostNodeId>,
    children: Vec<HostNodeId>,
    kind: UiNodeKind,
    layout_bounds: UiRect,
    paint_bounds: UiRect,
    interaction: InteractionFlags,
    node: UiNode,
}

struct HostSlot {
    generation: u32,
    node: Option<HostNode>,
}

#[derive(Clone)]
struct SceneNode {
    signature: u64,
    commands: Vec<ScenePrimitive>,
}

#[derive(Default)]
pub struct HostRuntime {
    slots: Vec<HostSlot>,
    free: Vec<u32>,
    sources: HashMap<UiId, HostNodeId>,
    paint_order: Vec<HostNodeId>,
    scene: HashMap<HostNodeId, SceneNode>,
    scene_order: Vec<HostNodeId>,
    scene_ranges: HashMap<HostNodeId, (usize, usize)>,
    composed_scene: Scene,
    initialized: bool,
}

impl HostRuntime {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn commit(
        &mut self,
        tree: &HostTree,
        interaction: &UiInteractionState,
        viewport: UiRect,
        invalidations: &mut InvalidationSet,
    ) -> HostCommit {
        let next_sources = tree
            .nodes()
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let changes = crate::core::ProjectionChanges {
            changed: next_sources.clone(),
            removed: self
                .sources
                .keys()
                .filter(|source| !next_sources.contains(*source))
                .cloned()
                .collect(),
            structure_changed: true,
            ..crate::core::ProjectionChanges::default()
        };
        self.commit_projection(tree, interaction, viewport, invalidations, changes)
    }

    pub(crate) fn commit_projection(
        &mut self,
        tree: &HostTree,
        interaction: &UiInteractionState,
        viewport: UiRect,
        invalidations: &mut InvalidationSet,
        changes: crate::core::ProjectionChanges,
    ) -> HostCommit {
        #[cfg(feature = "diagnostics-timing")]
        let change_scan_started = Instant::now();
        let mut mutations = Vec::new();
        let mut dirty_scene_sources = HashSet::new();
        let mut compositing_updates = HashMap::new();

        let kind_changed_sources = changes
            .changed
            .iter()
            .filter_map(|source| {
                let id = self.sources.get(source)?;
                tree.node(source)
                    .is_some_and(|next| self.node(*id).kind != next.kind)
                    .then(|| source.clone())
            })
            .collect::<Vec<_>>();
        let scene_topology_changed = changes.changed.iter().any(|source| {
            let Some(id) = self.sources.get(source) else {
                return false;
            };
            let Some(next) = tree.node(source) else {
                return false;
            };
            let previous = &self.node(*id).node;
            previous.parent != next.parent
                || previous.kind != next.kind
                || previous.render_phase != next.render_phase
        });
        let scene_structure_changed =
            changes.structure_changed || !changes.removed.is_empty() || scene_topology_changed;
        #[cfg(feature = "diagnostics-timing")]
        let change_scan_ms = elapsed_ms(change_scan_started);
        #[cfg(feature = "diagnostics-timing")]
        let node_patch_started = Instant::now();
        let mut removed_sources = changes.removed.clone();
        removed_sources.extend(kind_changed_sources);
        let removed = removed_sources
            .into_iter()
            .filter_map(|source| self.sources.get(&source).copied().map(|id| (source, id)))
            .collect::<Vec<_>>();
        // Snapshot every removal while the old parent graph is still intact.
        // A projection may remove a parent and its descendants in the same
        // commit, so releasing either one before this pass would leave stale
        // HostNodeIds in the remaining nodes' ancestry.
        for (source, id) in &removed {
            let old_bounds = self.node(*id).paint_bounds;
            if let Some(owner) = self.scene_owner_source(*id) {
                dirty_scene_sources.insert(owner);
            }
            mutations.push(HostMutation::RemoveNode {
                id: *id,
                source: source.clone(),
                old_bounds,
            });
        }
        for (_, id) in removed {
            self.remove(id);
        }

        let initializing = !self.initialized;
        let changed_nodes = if initializing {
            tree.nodes().iter().map(|node| node.as_ref()).collect()
        } else {
            tree.changed_nodes(&changes.changed)
        };
        for node in &changed_nodes {
            if !self.sources.contains_key(&node.id) {
                let id = self.allocate(node.id.clone());
                self.sources.insert(node.id.clone(), id);
            }
        }

        let order_changed = if changes.structure_changed || !self.initialized {
            let next_order = tree
                .nodes()
                .iter()
                .map(|node| self.sources[&node.id])
                .collect::<Vec<_>>();
            let changed = self.initialized && self.paint_order != next_order;
            self.paint_order = next_order;
            changed
        } else {
            false
        };

        for node in &changed_nodes {
            let id = self.sources[&node.id];
            let parent = node
                .parent
                .as_ref()
                .and_then(|parent| self.sources.get(parent))
                .copied();
            let children = node
                .children
                .iter()
                .filter_map(|child| self.sources.get(child))
                .copied()
                .collect::<Vec<_>>();
            let flags = interaction.flags_for(&node.id);
            let existing = self.node(id);
            let was_mounted = existing.mounted;
            let old_layout = existing.layout_bounds;
            let old_paint = existing.paint_bounds;
            let old_parent = existing.parent;
            let old_children = existing.children.clone();
            let old_interaction = existing.interaction;
            let next_paint = effective_node_paint_bounds(node);
            let compositing_only =
                was_mounted && compositing_spec_only_changed(&existing.node, node);

            if !was_mounted {
                mutations.push(HostMutation::InsertNode {
                    id,
                    source: node.id.clone(),
                    bounds: next_paint,
                });
            } else {
                if old_layout != node.layout_rect || old_paint != next_paint {
                    mutations.push(HostMutation::UpdateProps {
                        id,
                        source: node.id.clone(),
                        kind: HostUpdateKind::Layout,
                        old_bounds: old_paint,
                        new_bounds: next_paint,
                    });
                    if !compositing_only {
                        dirty_scene_sources.insert(node.id.clone());
                    }
                }
                if paint_props_changed(&existing.node, node) {
                    mutations.push(HostMutation::UpdateProps {
                        id,
                        source: node.id.clone(),
                        kind: HostUpdateKind::Paint,
                        old_bounds: old_paint,
                        new_bounds: next_paint,
                    });
                    if compositing_only {
                        compositing_updates.insert(
                            node.id.clone(),
                            node.compositing_layer.expect("compositing layer spec"),
                        );
                    } else {
                        dirty_scene_sources.insert(node.id.clone());
                    }
                }
                if old_interaction != flags {
                    mutations.push(HostMutation::UpdateProps {
                        id,
                        source: node.id.clone(),
                        kind: HostUpdateKind::Interaction,
                        old_bounds: old_paint,
                        new_bounds: next_paint,
                    });
                }
                if old_parent != parent || old_children != children {
                    let bounds = child_structure_damage(
                        self,
                        tree,
                        &old_children,
                        &children,
                        old_paint.union(next_paint),
                    );
                    mutations.push(HostMutation::ReorderChildren {
                        id,
                        source: node.id.clone(),
                        bounds,
                    });
                    dirty_scene_sources.insert(node.id.clone());
                }
            }

            if !was_mounted {
                dirty_scene_sources.insert(node.id.clone());
            }

            let current = self.node_mut(id);
            current.parent = parent;
            current.mounted = true;
            current.children = children;
            current.kind = node.kind;
            current.layout_bounds = node.layout_rect;
            current.paint_bounds = next_paint;
            current.interaction = flags;
            current.node = (*node).clone();
        }
        #[cfg(feature = "diagnostics-timing")]
        let node_patch_ms = elapsed_ms(node_patch_started);
        #[cfg(feature = "diagnostics-timing")]
        let scene_reconcile_started = Instant::now();
        let (scene_mutations, scene, reused_scene_nodes, compiled_scene_nodes, _scene_snapshot_ms) =
            self.reconcile_scene(
                tree,
                dirty_scene_sources,
                compositing_updates,
                order_changed,
                scene_structure_changed,
            );
        #[cfg(feature = "diagnostics-timing")]
        let scene_reconcile_ms =
            (elapsed_ms(scene_reconcile_started) - _scene_snapshot_ms).max(0.0);
        #[cfg(feature = "diagnostics-timing")]
        let damage_started = Instant::now();
        let damage = self.calculate_damage(viewport, &mutations, invalidations);
        #[cfg(feature = "diagnostics-timing")]
        let damage_ms = elapsed_ms(damage_started);
        #[cfg(feature = "diagnostics-timing")]
        let finalize_started = Instant::now();
        self.initialized = true;
        let metrics = HostCommitMetrics {
            host_nodes: self.sources.len(),
            scene_nodes: self.scene.len(),
            visited_host_nodes: changed_nodes.len(),
            compiled_scene_nodes,
            host_mutations: mutations.len(),
            scene_mutations: scene_mutations.len(),
            reused_scene_nodes,
        };
        #[cfg(feature = "diagnostics-timing")]
        let finalize_ms = elapsed_ms(finalize_started);
        HostCommit {
            mutations,
            scene_mutations,
            damage,
            scene,
            metrics,
            #[cfg(feature = "diagnostics-timing")]
            timings: HostCommitTimings {
                change_scan_ms,
                node_patch_ms,
                scene_reconcile_ms,
                scene_snapshot_ms: _scene_snapshot_ms,
                damage_ms,
                finalize_ms,
            },
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn scene_owner_source(&self, id: HostNodeId) -> Option<UiId> {
        let mut current = id;
        let mut owner = is_scene_drawable(self.node(current).kind).then_some(current);
        while let Some(parent) = self.node(current).parent {
            let current_node = self.node(current);
            let parent_node = self.node(parent);
            if current_node.node.render_phase == crate::core::RenderPhase::Popup
                && parent_node.node.render_phase != crate::core::RenderPhase::Popup
            {
                break;
            }
            if is_scene_container(parent_node.kind) {
                owner = Some(parent);
            }
            current = parent;
        }
        owner.map(|owner| self.node(owner).source.clone())
    }

    fn reconcile_scene(
        &mut self,
        tree: &HostTree,
        dirty_sources: HashSet<UiId>,
        compositing_updates: HashMap<UiId, CompositingLayerSpec>,
        host_order_changed: bool,
        scene_structure_changed: bool,
    ) -> (Vec<SceneMutation>, Scene, usize, usize, f32) {
        let retained_order = !scene_structure_changed
            && self.initialized
            && self
                .scene_order
                .iter()
                .all(|id| self.try_node(*id).is_some());
        let root_ids = if retained_order {
            self.scene_order.clone()
        } else {
            scene_root_ids(tree)
                .iter()
                .filter_map(|source| self.sources.get(source))
                .copied()
                .collect::<Vec<_>>()
        };
        let removed = if retained_order {
            Vec::new()
        } else {
            let live = root_ids.iter().copied().collect::<HashSet<_>>();
            self.scene
                .keys()
                .filter(|id| !live.contains(id))
                .copied()
                .collect::<Vec<_>>()
        };
        let mut mutations = removed
            .iter()
            .copied()
            .map(SceneMutation::Remove)
            .collect::<Vec<_>>();
        for id in removed {
            self.scene.remove(&id);
        }

        let mut dirty_roots = HashSet::new();
        for source in dirty_sources {
            if let Some(owner) = scene_owner_source_for_tree(tree, &source) {
                if let Some(id) = self.sources.get(&owner) {
                    dirty_roots.insert(*id);
                }
            }
        }

        let mut fast_updated = HashSet::new();
        let mut fast_specs = Vec::new();
        for (source, spec) in compositing_updates {
            let Some(owner_source) = scene_owner_source_for_tree(tree, &source) else {
                continue;
            };
            let Some(owner_id) = self.sources.get(&owner_source).copied() else {
                continue;
            };
            if dirty_roots.contains(&owner_id) {
                continue;
            }
            let Some(scene) = self.scene.get_mut(&owner_id) else {
                continue;
            };
            if patch_compositing_layer_spec(&mut scene.commands, &source, spec) {
                scene.signature = command_signature(&scene.commands);
                if fast_updated.insert(owner_id) {
                    mutations.push(SceneMutation::Update(owner_id));
                }
                fast_specs.push((source, spec));
            }
        }

        let mut reused = 0;
        let mut compiled = 0;
        for id in &root_ids {
            if self.scene.contains_key(id) && !dirty_roots.contains(id) {
                if !fast_updated.contains(id) {
                    reused += 1;
                }
                continue;
            }
            let source = self.node(*id).source.clone();
            let compiled_scene = compile_scene_root(tree, &source);
            compiled += 1;
            let commands = compiled_scene.commands().to_vec();
            let signature = command_signature(&commands);
            match self.scene.get_mut(id) {
                Some(scene) if scene.signature == signature => reused += 1,
                Some(scene) => {
                    scene.signature = signature;
                    scene.commands = commands;
                    mutations.push(SceneMutation::Update(*id));
                }
                None => {
                    self.scene.insert(
                        *id,
                        SceneNode {
                            signature,
                            commands,
                        },
                    );
                    mutations.push(SceneMutation::Insert(*id));
                }
            }
        }
        let next_scene_order = root_ids;
        if host_order_changed || self.scene_order != next_scene_order {
            mutations.push(SceneMutation::Reorder);
        }
        self.scene_order = next_scene_order;
        let structural_change = mutations.iter().any(|mutation| {
            matches!(
                mutation,
                SceneMutation::Insert(_) | SceneMutation::Remove(_) | SceneMutation::Reorder
            )
        }) || self.scene_ranges.len() != self.scene_order.len();
        if structural_change {
            let mut commands = Vec::new();
            let mut ranges = HashMap::new();
            for id in &self.scene_order {
                let start = commands.len();
                if let Some(scene_node) = self.scene.get(id) {
                    commands.extend(scene_node.commands.iter().cloned());
                }
                ranges.insert(*id, (start, commands.len()));
            }
            self.composed_scene.replace_all(commands);
            self.scene_ranges = ranges;
        } else {
            let updated = mutations
                .iter()
                .filter_map(|mutation| match mutation {
                    SceneMutation::Update(id) if !fast_updated.contains(id) => Some(*id),
                    _ => None,
                })
                .collect::<HashSet<_>>();
            if !updated.is_empty() {
                let previous_ranges = std::mem::take(&mut self.scene_ranges);
                let mut next_ranges = HashMap::with_capacity(previous_ranges.len());
                let mut offset = 0_isize;
                for id in &self.scene_order {
                    let (old_start, old_end) = previous_ranges[id];
                    let start = (old_start as isize + offset) as usize;
                    let end = (old_end as isize + offset) as usize;
                    if updated.contains(id) {
                        let commands = self
                            .scene
                            .get(id)
                            .map(|node| node.commands.clone())
                            .unwrap_or_default();
                        let next_end = start + commands.len();
                        self.composed_scene.replace_range(start..end, commands);
                        offset += next_end as isize - end as isize;
                        next_ranges.insert(*id, (start, next_end));
                    } else {
                        next_ranges.insert(*id, (start, end));
                    }
                }
                self.scene_ranges = next_ranges;
            }
        }
        for (source, spec) in fast_specs {
            let patched = self
                .composed_scene
                .patch_compositing_layer_spec(&source, spec);
            debug_assert!(patched, "retained compositing command must exist");
        }
        #[cfg(feature = "diagnostics-timing")]
        let snapshot_started = Instant::now();
        let scene = self.composed_scene.clone();
        #[cfg(feature = "diagnostics-timing")]
        let snapshot_ms = elapsed_ms(snapshot_started);
        #[cfg(not(feature = "diagnostics-timing"))]
        let snapshot_ms = 0.0;
        (mutations, scene, reused, compiled, snapshot_ms)
    }

    fn calculate_damage(
        &self,
        viewport: UiRect,
        mutations: &[HostMutation],
        invalidations: &mut InvalidationSet,
    ) -> DamageReport {
        let mut dirty = DirtyRegionSet::new(viewport);
        let mut reasons = Vec::new();
        let mut details = Vec::new();
        if !self.initialized {
            dirty.mark_full_with_reason("first-commit");
            reasons.push(DamageReason::FirstCommit);
            details.push(DamageDetail {
                reason: DamageReason::FirstCommit,
                node_id: None,
                old_bounds: None,
                new_bounds: Some(viewport),
                rects: vec![viewport],
            });
        }
        for mutation in mutations {
            match mutation {
                HostMutation::InsertNode { source, bounds, .. } => {
                    dirty.add(*bounds);
                    reasons.push(DamageReason::Insert);
                    details.push(DamageDetail {
                        reason: DamageReason::Insert,
                        node_id: Some(source.clone()),
                        old_bounds: None,
                        new_bounds: Some(*bounds),
                        rects: vec![*bounds],
                    });
                }
                HostMutation::RemoveNode {
                    source, old_bounds, ..
                } => {
                    dirty.add(*old_bounds);
                    reasons.push(DamageReason::Remove);
                    details.push(DamageDetail {
                        reason: DamageReason::Remove,
                        node_id: Some(source.clone()),
                        old_bounds: Some(*old_bounds),
                        new_bounds: None,
                        rects: vec![*old_bounds],
                    });
                }
                HostMutation::UpdateProps {
                    source,
                    kind,
                    old_bounds,
                    new_bounds,
                    ..
                } => {
                    dirty.add(*old_bounds);
                    dirty.add(*new_bounds);
                    let reason = match kind {
                        HostUpdateKind::Layout => DamageReason::Layout,
                        HostUpdateKind::Paint => DamageReason::Paint,
                        HostUpdateKind::Interaction => DamageReason::Interaction,
                    };
                    reasons.push(reason);
                    details.push(DamageDetail {
                        reason,
                        node_id: Some(source.clone()),
                        old_bounds: Some(*old_bounds),
                        new_bounds: Some(*new_bounds),
                        rects: vec![*old_bounds, *new_bounds],
                    });
                }
                HostMutation::ReorderChildren { source, bounds, .. } => {
                    dirty.add(*bounds);
                    reasons.push(DamageReason::Structure);
                    details.push(DamageDetail {
                        reason: DamageReason::Structure,
                        node_id: Some(source.clone()),
                        old_bounds: Some(*bounds),
                        new_bounds: Some(*bounds),
                        rects: vec![*bounds],
                    });
                }
            }
        }
        for request in invalidations.drain() {
            match request {
                InvalidationRequest::All | InvalidationRequest::Route => {
                    dirty.mark_full_with_reason("explicit");
                }
                InvalidationRequest::Rect(rect) => dirty.add(rect),
                InvalidationRequest::Node(source)
                | InvalidationRequest::Animation { id: source, .. } => {
                    if let Some(id) = self.sources.get(&source) {
                        dirty.add(self.node(*id).paint_bounds);
                    }
                }
            }
            reasons.push(DamageReason::Explicit);
        }
        if reasons.is_empty() {
            reasons.push(DamageReason::Clean);
            details.push(DamageDetail {
                reason: DamageReason::Clean,
                node_id: None,
                old_bounds: None,
                new_bounds: None,
                rects: Vec::new(),
            });
        }
        DamageReport {
            dirty,
            reasons,
            details,
        }
    }

    fn allocate(&mut self, source: UiId) -> HostNodeId {
        let index = self.free.pop().unwrap_or(self.slots.len() as u32);
        if index as usize == self.slots.len() {
            self.slots.push(HostSlot {
                generation: 0,
                node: None,
            });
        }
        let generation = self.slots[index as usize].generation;
        let id = HostNodeId { index, generation };
        self.slots[index as usize].node = Some(HostNode {
            source: source.clone(),
            mounted: false,
            parent: None,
            children: Vec::new(),
            kind: UiNodeKind::Root,
            layout_bounds: UiRect::new(0, 0, 0, 0),
            paint_bounds: UiRect::new(0, 0, 0, 0),
            interaction: InteractionFlags::default(),
            node: UiNode::new(source, UiNodeKind::Root, UiRect::new(0, 0, 0, 0)),
        });
        id
    }

    fn remove(&mut self, id: HostNodeId) {
        let slot = &mut self.slots[id.index as usize];
        let Some(node) = slot.node.take() else {
            return;
        };
        self.sources.remove(&node.source);
        self.scene.remove(&id);
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
    }

    fn node(&self, id: HostNodeId) -> &HostNode {
        self.try_node(id)
            .unwrap_or_else(|| panic!("stale host node id `{id}`"))
    }

    fn try_node(&self, id: HostNodeId) -> Option<&HostNode> {
        self.slots
            .get(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_ref())
    }

    fn node_mut(&mut self, id: HostNodeId) -> &mut HostNode {
        self.slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
            .unwrap_or_else(|| panic!("stale host node id `{id}`"))
    }
}

fn effective_node_paint_bounds(node: &UiNode) -> UiRect {
    let bounds = node
        .compositing_layer
        .map(|spec| spec.transform.transformed_bounds(node.layout_rect))
        .unwrap_or(node.paint_bounds);
    bounds.inflate(node.animation_outset.0, node.animation_outset.1)
}

fn compositing_spec_only_changed(previous: &UiNode, next: &UiNode) -> bool {
    if previous.compositing_layer == next.compositing_layer
        || previous.compositing_layer.is_none()
        || next.compositing_layer.is_none()
    {
        return false;
    }
    let mut normalized = previous.clone();
    normalized.compositing_layer = next.compositing_layer;
    normalized.projection_eq(next)
}

fn command_signature(commands: &[ScenePrimitive]) -> u64 {
    commands.iter().fold(0, |signature, command| {
        signature.rotate_left(7) ^ command.signature()
    })
}

fn scene_owner_source_for_tree(tree: &HostTree, source: &UiId) -> Option<UiId> {
    let mut current = tree.node(source)?;
    let mut owner = is_scene_drawable(current.kind).then(|| current.id.clone());
    while let Some(parent_id) = current.parent.as_ref() {
        let Some(parent) = tree.node(parent_id) else {
            break;
        };
        if current.render_phase == crate::core::RenderPhase::Popup
            && parent.render_phase != crate::core::RenderPhase::Popup
        {
            break;
        }
        if is_scene_container(parent.kind) {
            owner = Some(parent.id.clone());
        }
        current = parent;
    }
    owner
}

fn is_scene_container(kind: UiNodeKind) -> bool {
    matches!(
        kind,
        UiNodeKind::CompositingLayer
            | UiNodeKind::StaticLayer
            | UiNodeKind::ScrollRaster
            | UiNodeKind::Clip
            | UiNodeKind::ClipPath
    )
}

fn is_scene_drawable(kind: UiNodeKind) -> bool {
    !matches!(kind, UiNodeKind::Root | UiNodeKind::Group)
}

fn child_structure_damage(
    host: &HostRuntime,
    tree: &HostTree,
    previous: &[HostNodeId],
    next: &[HostNodeId],
    fallback: UiRect,
) -> UiRect {
    let previous_positions = previous
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<HashMap<_, _>>();
    let next_positions = next
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<HashMap<_, _>>();
    let mut affected = previous
        .iter()
        .chain(next)
        .copied()
        .filter(|id| previous_positions.get(id) != next_positions.get(id))
        .collect::<HashSet<_>>();
    let mut bounds = None;
    for id in affected.drain() {
        let Some(node) = host.try_node(id) else {
            continue;
        };
        if node.mounted {
            bounds = Some(bounds.map_or(node.paint_bounds, |bounds: UiRect| {
                bounds.union(node.paint_bounds)
            }));
        }
        if let Some(next) = tree.node(&node.source) {
            bounds = Some(bounds.map_or(next.paint_bounds, |bounds: UiRect| {
                bounds.union(next.paint_bounds)
            }));
        }
    }
    bounds.unwrap_or(fallback)
}

fn paint_props_changed(previous: &UiNode, next: &UiNode) -> bool {
    previous.kind != next.kind
        || previous.layout_rect != next.layout_rect
        || previous.paint_bounds != next.paint_bounds
        || previous.style != next.style
        || previous.path != next.path
        || previous.path_style != next.path_style
        || previous.image_source != next.image_source
        || previous.image_fit != next.image_fit
        || previous.icon_key != next.icon_key
        || previous.icon_style != next.icon_style
        || previous.glow != next.glow
        || previous.backdrop_blur_style != next.backdrop_blur_style
        || previous.overlay_style != next.overlay_style
        || previous.custom_style != next.custom_style
        || previous.compositing_layer != next.compositing_layer
        || previous.static_layer != next.static_layer
        || previous.scroll_raster != next.scroll_raster
        || previous.clip_rect != next.clip_rect
        || previous.content_offset != next.content_offset
        || previous.text != next.text
        || previous.text_style != next.text_style
        || previous.render_phase != next.render_phase
}

#[cfg(feature = "diagnostics-timing")]
fn elapsed_ms(started: Instant) -> f32 {
    started.elapsed().as_secs_f32() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CompositingLayerSpec, Point, UiNode, VisualStyle};

    fn compositing_content_signature(scene: &Scene, id: &UiId) -> u64 {
        scene
            .commands()
            .iter()
            .find_map(|command| match command {
                ScenePrimitive::CompositingLayer {
                    id: command_id,
                    content_signature,
                    ..
                } if command_id == id => Some(*content_signature),
                _ => None,
            })
            .expect("compositing layer command")
    }

    fn compositing_child_storage(scene: &Scene, id: &UiId) -> usize {
        scene
            .commands()
            .iter()
            .find_map(|command| match command {
                ScenePrimitive::CompositingLayer {
                    id: command_id,
                    commands,
                    ..
                } if command_id == id => Some(commands.as_ptr() as usize),
                _ => None,
            })
            .expect("compositing layer command")
    }

    #[test]
    fn moving_a_node_damages_old_and_new_bounds_without_a_frame_snapshot() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 500, 500);
        let id = UiId::owned("moving");
        let mut first = HostTree::new();
        first.push(
            UiNode::new(id.clone(), UiNodeKind::Panel, UiRect::new(10, 10, 40, 40))
                .style(VisualStyle::default()),
        );
        host.commit(&first, &interaction, viewport, &mut InvalidationSet::new());

        let mut second = HostTree::new();
        second.push(UiNode::new(
            id,
            UiNodeKind::Panel,
            UiRect::new(100, 100, 130, 130),
        ));
        let commit = host.commit(&second, &interaction, viewport, &mut InvalidationSet::new());

        assert!(!commit.damage.dirty.is_empty());
        assert!(commit.mutations.iter().any(|mutation| matches!(
            mutation,
            HostMutation::UpdateProps {
                kind: HostUpdateKind::Layout,
                ..
            }
        )));
    }

    #[test]
    fn moving_layer_damages_old_and_new_bounds_without_recompiling_siblings() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 400, 300);
        let layer_id = UiId::owned("moving-layer");
        let child_id = UiId::owned("moving-layer-child");
        let make_tree = |layer_rect: UiRect, child_rect: UiRect| {
            let mut tree = HostTree::new();
            tree.push(
                UiNode::new(UiId::owned("background"), UiNodeKind::Panel, viewport)
                    .style(VisualStyle::filled(crate::core::Color::BLACK)),
            );
            tree.push(
                UiNode::new(layer_id.clone(), UiNodeKind::CompositingLayer, layer_rect)
                    .compositing_layer(CompositingLayerSpec::new()),
            );
            tree.push(
                UiNode::new(child_id.clone(), UiNodeKind::Panel, child_rect)
                    .parent(layer_id.clone())
                    .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
            tree.push(
                UiNode::new(UiId::owned("foreground"), UiNodeKind::Panel, viewport)
                    .style(VisualStyle::default()),
            );
            tree
        };

        host.commit(
            &make_tree(UiRect::new(10, 20, 90, 100), UiRect::new(20, 30, 40, 50)),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );
        let commit = host.commit(
            &make_tree(
                UiRect::new(200, 160, 280, 240),
                UiRect::new(210, 170, 230, 190),
            ),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );

        let dirty = commit.damage.dirty.effective_rects();
        assert!(dirty.iter().any(|rect| rect.contains(Point::new(20, 30))));
        assert!(dirty.iter().any(|rect| rect.contains(Point::new(210, 170))));
        assert!(
            !dirty.iter().any(|rect| rect.contains(Point::new(150, 130))),
            "moving a retained layer must not dirty the area between old and new bounds"
        );
        assert_eq!(commit.metrics.reused_scene_nodes, 2);
        assert_eq!(
            commit
                .scene_mutations
                .iter()
                .filter(|mutation| matches!(mutation, SceneMutation::Update(_)))
                .count(),
            1
        );
    }

    #[test]
    fn shrinking_a_transformed_layer_damages_pixels_outside_its_layout_rect() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 200, 200);
        let make_tree = |scale| {
            let mut tree = HostTree::new();
            tree.push(
                UiNode::new(
                    UiId::owned("transformed-layer"),
                    UiNodeKind::CompositingLayer,
                    UiRect::new(40, 40, 120, 120),
                )
                .compositing_layer(CompositingLayerSpec::new().scale(scale)),
            );
            tree.push(
                UiNode::new(
                    UiId::owned("transformed-child"),
                    UiNodeKind::Panel,
                    UiRect::new(50, 50, 110, 110),
                )
                .parent(UiId::owned("transformed-layer"))
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
            tree
        };

        host.commit(
            &make_tree(1.2),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );
        let commit = host.commit(
            &make_tree(1.0),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );

        let dirty = commit.damage.dirty.effective_rects();
        assert!(dirty.iter().any(|rect| rect.contains(Point::new(33, 80))));
        assert!(dirty.iter().any(|rect| rect.contains(Point::new(80, 80))));
    }

    #[test]
    fn composition_only_update_patches_the_retained_scene_without_compiling_children() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 200, 200);
        let layer_id = UiId::owned("retained-transform-layer");
        let child_id = UiId::owned("retained-transform-child");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                layer_id.clone(),
                UiNodeKind::CompositingLayer,
                UiRect::new(0, 0, 40, 40),
            )
            .compositing_layer(CompositingLayerSpec::new()),
        );
        tree.push(
            UiNode::new(
                child_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(5, 5, 35, 35),
            )
            .parent(layer_id.clone())
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        let changes = tree.take_projection_changes();
        let first = host.commit_projection(
            &tree,
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
            changes,
        );
        let first_signature = compositing_content_signature(&first.scene, &layer_id);
        let first_child_storage = compositing_child_storage(&first.scene, &layer_id);
        drop(first);

        tree.node_mut(&child_id).expect("child").style =
            VisualStyle::filled(crate::core::Color(0xFF0000));
        assert!(tree
            .update_compositing_layer(
                &layer_id,
                CompositingLayerSpec::new().translation(50.0, 25.0),
            )
            .is_some());
        let changes = tree.take_projection_changes();
        assert_eq!(changes.changed.len(), 1);
        let second = host.commit_projection(
            &tree,
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
            changes,
        );

        assert_eq!(second.metrics.visited_host_nodes, 1);
        assert_eq!(second.metrics.compiled_scene_nodes, 0);

        assert_eq!(
            compositing_content_signature(&second.scene, &layer_id),
            first_signature
        );
        assert_eq!(
            compositing_child_storage(&second.scene, &layer_id),
            first_child_storage,
            "composition-only updates must retain static child command storage"
        );
        let ScenePrimitive::CompositingLayer { spec, .. } = second
            .scene
            .commands()
            .iter()
            .find(|command| command.id() == &layer_id)
            .expect("compositing command")
        else {
            panic!("expected compositing command");
        };
        assert_eq!(
            (
                spec.transform.translation_x(),
                spec.transform.translation_y()
            ),
            (50.0, 25.0)
        );
    }

    #[test]
    fn inserting_content_into_a_large_layer_keeps_window_damage_local() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 800, 600);
        let layer_id = UiId::owned("large-layer");
        let make_tree = |include_inserted: bool| {
            let mut tree = HostTree::new();
            tree.push(
                UiNode::new(layer_id.clone(), UiNodeKind::CompositingLayer, viewport)
                    .compositing_layer(CompositingLayerSpec::new()),
            );
            tree.push(
                UiNode::new(
                    UiId::owned("stable-child"),
                    UiNodeKind::Panel,
                    UiRect::new(20, 20, 80, 80),
                )
                .parent(layer_id.clone())
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
            if include_inserted {
                tree.push(
                    UiNode::new(
                        UiId::owned("inserted-child"),
                        UiNodeKind::Panel,
                        UiRect::new(120, 100, 180, 160),
                    )
                    .parent(layer_id.clone())
                    .style(VisualStyle::filled(crate::core::Color::WHITE)),
                );
            }
            tree
        };

        host.commit(
            &make_tree(false),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );
        let commit = host.commit(
            &make_tree(true),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );

        let damage = commit.damage.dirty.effective_rects();
        assert!(damage
            .iter()
            .any(|rect| rect.contains(Point::new(140, 120))));
        assert!(!damage
            .iter()
            .any(|rect| rect.contains(Point::new(700, 500))));
    }

    #[test]
    fn removing_and_reinserting_uses_a_new_generation() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 100, 100);
        let mut tree = HostTree::new();
        tree.push(UiNode::new(
            UiId::owned("node"),
            UiNodeKind::Group,
            viewport,
        ));
        let first = host.commit(&tree, &interaction, viewport, &mut InvalidationSet::new());
        let first_id = match &first.mutations[0] {
            HostMutation::InsertNode { id, .. } => *id,
            mutation => panic!("unexpected mutation: {mutation:?}"),
        };
        host.commit(
            &HostTree::new(),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );
        let second = host.commit(&tree, &interaction, viewport, &mut InvalidationSet::new());
        let second_id = match &second.mutations[0] {
            HostMutation::InsertNode { id, .. } => *id,
            mutation => panic!("unexpected mutation: {mutation:?}"),
        };
        assert_ne!(first_id, second_id);
    }

    #[test]
    fn removing_a_parent_and_its_child_in_one_commit_keeps_the_old_graph_readable() {
        for iteration in 0..32 {
            let mut host = HostRuntime::new();
            let interaction = UiInteractionState::default();
            let viewport = UiRect::new(0, 0, 100, 100);
            let parent = UiId::owned(format!("parent-{iteration}"));
            let child = UiId::owned(format!("child-{iteration}"));
            let mut tree = HostTree::new();
            tree.push(UiNode::new(parent.clone(), UiNodeKind::Clip, viewport).clip(viewport, 0, 0));
            tree.push(
                UiNode::new(child, UiNodeKind::Panel, viewport)
                    .parent(parent)
                    .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
            host.commit(&tree, &interaction, viewport, &mut InvalidationSet::new());

            let commit = host.commit(
                &HostTree::new(),
                &interaction,
                viewport,
                &mut InvalidationSet::new(),
            );

            assert_eq!(
                commit
                    .mutations
                    .iter()
                    .filter(|mutation| matches!(mutation, HostMutation::RemoveNode { .. }))
                    .count(),
                2
            );
        }
    }

    #[test]
    fn paint_change_recompiles_only_its_retained_scene_root() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 200, 100);
        let first_id = UiId::owned("first");
        let second_id = UiId::owned("second");
        let make_tree = |first_fill| {
            let mut tree = HostTree::new();
            tree.push(
                UiNode::new(
                    first_id.clone(),
                    UiNodeKind::Panel,
                    UiRect::new(0, 0, 100, 100),
                )
                .style(VisualStyle::filled(first_fill)),
            );
            tree.push(
                UiNode::new(
                    second_id.clone(),
                    UiNodeKind::Panel,
                    UiRect::new(100, 0, 200, 100),
                )
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
            tree
        };
        host.commit(
            &make_tree(crate::core::Color::BLACK),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );
        let commit = host.commit(
            &make_tree(crate::core::Color::WHITE),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );

        assert_eq!(commit.metrics.reused_scene_nodes, 1);
        assert_eq!(
            commit
                .scene_mutations
                .iter()
                .filter(|mutation| matches!(mutation, SceneMutation::Update(_)))
                .count(),
            1
        );
    }

    #[test]
    fn nested_clip_change_recompiles_only_the_clip_scene_root() {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 240, 120);
        let clip_id = UiId::owned("clip");
        let child_id = UiId::owned("clip-child");
        let sibling_id = UiId::owned("sibling");
        let make_tree = |fill| {
            let mut tree = HostTree::new();
            tree.push(
                UiNode::new(
                    clip_id.clone(),
                    UiNodeKind::Clip,
                    UiRect::new(0, 0, 120, 120),
                )
                .clip(UiRect::new(0, 0, 120, 120), 0, 0),
            );
            tree.push(
                UiNode::new(
                    child_id.clone(),
                    UiNodeKind::Panel,
                    UiRect::new(0, 0, 120, 120),
                )
                .parent(clip_id.clone())
                .style(VisualStyle::filled(fill)),
            );
            tree.push(
                UiNode::new(
                    sibling_id.clone(),
                    UiNodeKind::Panel,
                    UiRect::new(120, 0, 240, 120),
                )
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
            tree
        };
        host.commit(
            &make_tree(crate::core::Color::BLACK),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );
        let next = make_tree(crate::core::Color::WHITE);
        let commit = host.commit(&next, &interaction, viewport, &mut InvalidationSet::new());

        assert_eq!(commit.metrics.reused_scene_nodes, 1);
        assert_eq!(
            commit
                .scene_mutations
                .iter()
                .filter(|mutation| matches!(mutation, SceneMutation::Update(_)))
                .count(),
            1
        );
        assert_eq!(commit.scene.commands(), next.scene().commands());
    }

    #[test]
    fn popup_escapes_ancestor_clip_and_remains_last_in_incremental_scene() {
        let mut tree = HostTree::new();
        let clip_id = UiId::owned("clip");
        tree.push(
            UiNode::new(
                clip_id.clone(),
                UiNodeKind::Clip,
                UiRect::new(0, 0, 100, 100),
            )
            .clip(UiRect::new(0, 0, 100, 100), 0, 0),
        );
        tree.push(
            UiNode::new(
                UiId::owned("clipped-content"),
                UiNodeKind::Panel,
                UiRect::new(0, 0, 100, 100),
            )
            .parent(clip_id.clone())
            .style(VisualStyle::filled(crate::core::Color::BLACK)),
        );
        tree.push(
            UiNode::new(
                UiId::owned("popup"),
                UiNodeKind::Panel,
                UiRect::new(80, 80, 180, 180),
            )
            .parent(clip_id)
            .render_phase(crate::core::RenderPhase::Popup)
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        let mut host = HostRuntime::new();
        let commit = host.commit(
            &tree,
            &UiInteractionState::default(),
            UiRect::new(0, 0, 200, 200),
            &mut InvalidationSet::new(),
        );

        assert_eq!(commit.scene.commands(), tree.scene().commands());
        assert_eq!(
            commit.scene.commands().last().map(ScenePrimitive::id),
            Some(&UiId::owned("popup"))
        );
    }

    #[test]
    fn render_phase_change_rebuilds_retained_scene_root_order() {
        let first_id = UiId::owned("first");
        let popup_id = UiId::owned("becomes-popup");
        let last_id = UiId::owned("last");
        let mut tree = HostTree::new();
        for id in [&first_id, &popup_id, &last_id] {
            tree.push(
                UiNode::new(id.clone(), UiNodeKind::Panel, UiRect::new(0, 0, 20, 20))
                    .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
        }
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0, 0, 20, 20);
        let changes = tree.take_projection_changes();
        host.commit_projection(
            &tree,
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
            changes,
        );

        let next = tree
            .node(&popup_id)
            .expect("phase-changing node")
            .clone()
            .render_phase(crate::core::RenderPhase::Popup);
        tree.upsert(next);
        let changes = tree.take_projection_changes();
        assert!(!changes.structure_changed);
        let commit = host.commit_projection(
            &tree,
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
            changes,
        );

        assert!(commit.scene_mutations.contains(&SceneMutation::Reorder));
        assert_eq!(
            commit.scene.commands().last().map(ScenePrimitive::id),
            Some(&popup_id)
        );
    }
}
