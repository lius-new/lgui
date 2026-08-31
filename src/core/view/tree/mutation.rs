use super::*;

impl HostTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            node_indices: Arc::new(HashMap::new()),
            owners: Arc::new(HashMap::new()),
            projection_changes: ProjectionChanges::default(),
        }
    }

    pub fn push(&mut self, node: UiNode) {
        self.projection_changes.changed.insert(node.id.clone());
        self.projection_changes.structure_changed = true;
        self.mark_runtime_sync_for_insert(&node);
        if let Some(parent_id) = node.parent.as_ref() {
            if let Some(parent) = self.node_mut(parent_id) {
                parent.children.push(node.id.clone());
            }
        }
        if let Some(owner) = node.component_owner {
            Arc::make_mut(&mut self.owners)
                .entry(owner)
                .or_default()
                .insert(node.id.clone());
        }
        let index = self.nodes.len();
        Arc::make_mut(&mut self.node_indices).insert(node.id.clone(), index);
        self.nodes.push(Arc::new(node));
    }

    pub(crate) fn upsert(&mut self, node: UiNode) {
        if let Some(index) = self.node_indices.get(&node.id).copied() {
            if !self.nodes[index].projection_eq(&node) {
                self.projection_changes.changed.insert(node.id.clone());
            }
            let previous = Arc::clone(&self.nodes[index]);
            self.mark_runtime_sync_for_update(&previous, &node);
            let previous_owner = self.nodes[index].component_owner;
            if previous_owner != node.component_owner {
                if let Some(owner) = previous_owner {
                    if let Some(ids) = Arc::make_mut(&mut self.owners).get_mut(&owner) {
                        ids.remove(&node.id);
                    }
                }
                if let Some(owner) = node.component_owner {
                    Arc::make_mut(&mut self.owners)
                        .entry(owner)
                        .or_default()
                        .insert(node.id.clone());
                }
            }
            self.nodes[index] = Arc::new(node);
        } else {
            self.push(node);
        }
    }

    pub(crate) fn set_children(&mut self, parent: &UiId, children: Vec<UiId>) {
        let changed = {
            let Some(parent) = self.node_mut(parent) else {
                return;
            };
            if parent.children == children {
                false
            } else {
                parent.children = children;
                true
            }
        };
        if changed {
            self.projection_changes.changed.insert(parent.clone());
            self.projection_changes.structure_changed = true;
        }
    }

    pub(crate) fn retain_owner_nodes(
        &mut self,
        owner: ComponentId,
        keep: &std::collections::HashSet<UiId>,
    ) -> bool {
        let remove = self
            .owners
            .get(&owner)
            .into_iter()
            .flatten()
            .filter(|id| !keep.contains(*id))
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        self.remove_ids(&remove)
    }

    pub(crate) fn prune_dead_component_owners(&mut self, components: &ComponentTree) -> bool {
        let remove = self
            .owners
            .keys()
            .copied()
            .filter(|owner| !components.is_alive(*owner))
            .flat_map(|owner| self.owners.get(&owner).into_iter().flatten().cloned())
            .collect::<std::collections::HashSet<_>>();
        self.remove_ids(&remove)
    }

    fn remove_ids(&mut self, remove: &std::collections::HashSet<UiId>) -> bool {
        if remove.is_empty() {
            return false;
        }
        for id in remove {
            if let Some(index) = self.node_indices.get(id).copied() {
                let node = Arc::clone(&self.nodes[index]);
                self.mark_runtime_sync_for_remove(&node);
            }
        }
        self.nodes.retain(|node| !remove.contains(&node.id));
        self.projection_changes
            .removed
            .extend(remove.iter().cloned());
        self.projection_changes.structure_changed = true;
        for node in &mut self.nodes {
            let children_changed = node.children.iter().any(|child| remove.contains(child));
            let parent_changed = node
                .parent
                .as_ref()
                .is_some_and(|parent| remove.contains(parent));
            if !children_changed && !parent_changed {
                continue;
            }
            let node = Arc::make_mut(node);
            if children_changed {
                node.children.retain(|child| !remove.contains(child));
            }
            if parent_changed {
                node.parent = None;
            }
        }
        Arc::make_mut(&mut self.owners).retain(|_, ids| {
            ids.retain(|id| !remove.contains(id));
            !ids.is_empty()
        });
        self.rebuild_node_indices();
        true
    }

    pub(crate) fn reorder_by_hierarchy(&mut self) {
        let previous_order = self
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        let nodes = self
            .nodes
            .drain(..)
            .map(|node| (node.id.clone(), node))
            .collect::<std::collections::HashMap<_, _>>();
        let roots = previous_order
            .iter()
            .filter(|id| nodes.get(*id).is_some_and(|node| node.parent.is_none()))
            .cloned()
            .collect::<Vec<_>>();
        let mut ordered = Vec::with_capacity(nodes.len());
        let mut seen = std::collections::HashSet::new();
        fn append(
            id: &UiId,
            nodes: &std::collections::HashMap<UiId, Arc<UiNode>>,
            seen: &mut std::collections::HashSet<UiId>,
            ordered: &mut Vec<Arc<UiNode>>,
        ) {
            if !seen.insert(id.clone()) {
                return;
            }
            let Some(node) = nodes.get(id) else {
                return;
            };
            ordered.push(node.clone());
            for child in &node.children {
                append(child, nodes, seen, ordered);
            }
        }
        for root in roots {
            append(&root, &nodes, &mut seen, &mut ordered);
        }
        for id in nodes.keys() {
            append(id, &nodes, &mut seen, &mut ordered);
        }
        self.nodes = ordered;
        self.rebuild_node_indices();
    }

    pub(crate) fn take_projection_changes(&mut self) -> ProjectionChanges {
        std::mem::take(&mut self.projection_changes)
    }

    pub fn nodes(&self) -> &[Arc<UiNode>] {
        &self.nodes
    }

    pub fn node(&self, id: &UiId) -> Option<&UiNode> {
        self.node_indices
            .get(id)
            .and_then(|index| self.nodes.get(*index))
            .map(Arc::as_ref)
    }

    pub fn node_mut(&mut self, id: &UiId) -> Option<&mut UiNode> {
        let index = self.node_indices.get(id).copied()?;
        self.nodes.get_mut(index).map(Arc::make_mut)
    }

    pub(crate) fn changed_nodes(&self, changed: &HashSet<UiId>) -> Vec<&UiNode> {
        let mut indices = changed
            .iter()
            .filter_map(|id| self.node_indices.get(id).copied())
            .collect::<Vec<_>>();
        indices.sort_unstable();
        indices
            .into_iter()
            .filter_map(|index| self.nodes.get(index).map(Arc::as_ref))
            .collect()
    }

    pub(crate) fn animation_sync_ids(&self) -> impl Iterator<Item = &UiId> {
        self.projection_changes.animation_sync.iter()
    }

    pub(crate) fn needs_focus_sync(&self) -> bool {
        self.projection_changes.focus_sync
    }

    pub(crate) fn update_compositing_layer(
        &mut self,
        id: &UiId,
        spec: CompositingLayerSpec,
    ) -> Option<UiRect> {
        let index = self.node_indices.get(id).copied()?;
        let node = Arc::make_mut(&mut self.nodes[index]);
        let previous = node.compositing_layer?;
        if previous == spec {
            return None;
        }

        let visible_bounds = |spec: CompositingLayerSpec| {
            (spec.opacity > 0).then(|| {
                spec.transform
                    .transformed_bounds(node.layout_rect)
                    .inflate(node.animation_outset.0, node.animation_outset.1)
            })
        };
        let old_bounds = visible_bounds(previous);
        let new_bounds = visible_bounds(spec);
        node.compositing_layer = Some(spec);
        self.projection_changes.changed.insert(id.clone());

        match (old_bounds, new_bounds) {
            (Some(old), Some(new)) => Some(old.union(new)),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => None,
        }
    }

    fn rebuild_node_indices(&mut self) {
        self.node_indices = Arc::new(
            self.nodes
                .iter()
                .enumerate()
                .map(|(index, node)| (node.id.clone(), index))
                .collect(),
        );
    }

    fn mark_runtime_sync_for_insert(&mut self, node: &UiNode) {
        if node_needs_animation_sync(node) {
            self.projection_changes
                .animation_sync
                .insert(node.id.clone());
        }
        self.projection_changes.focus_sync |= node_affects_focus(node);
    }

    fn mark_runtime_sync_for_update(&mut self, previous: &UiNode, next: &UiNode) {
        if previous.animation_bindings != next.animation_bindings
            || previous.animation_targets != next.animation_targets
        {
            self.projection_changes
                .animation_sync
                .insert(next.id.clone());
        }
        self.projection_changes.focus_sync |= !focus_projection_eq(previous, next);
    }

    fn mark_runtime_sync_for_remove(&mut self, node: &UiNode) {
        if node_needs_animation_sync(node) {
            self.projection_changes
                .animation_sync
                .insert(node.id.clone());
        }
        self.projection_changes.focus_sync |= node_affects_focus(node);
    }
}

fn node_needs_animation_sync(node: &UiNode) -> bool {
    !node.animation_bindings.is_empty() || !node.animation_targets.is_empty()
}

fn node_affects_focus(node: &UiNode) -> bool {
    node.event_policy.focus || node.auto_focus || node.focus_scope
}

fn focus_projection_eq(previous: &UiNode, next: &UiNode) -> bool {
    previous.parent == next.parent
        && previous.render_phase == next.render_phase
        && previous.event_policy.focus == next.event_policy.focus
        && previous.auto_focus == next.auto_focus
        && previous.focus_scope == next.focus_scope
}
