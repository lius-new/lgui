use super::{input::event_target_ids, *};

impl UiRuntime {
    pub fn advance(&mut self, tree: &mut HostTree, elapsed_ms: f32) -> RuntimeOutput {
        let animation_changed = self.animations.advance(elapsed_ms);
        let mut frame_interval_ms = animation_changed.then_some(16);
        let animation_ids = self.animations.take_dirty_ids();
        self.mark_component_owners(tree, animation_ids.iter());
        self.dirty.mark_animation_ids(animation_ids);
        let component_invalidations = self.component_states.advance_invalidations(elapsed_ms);
        let component_changed = !component_invalidations.is_empty();
        let mut route_changed = false;
        let mut retained_dirty_bounds: Option<UiRect> = None;
        let mut regular_dirty_ids = Vec::new();
        for invalidation in &component_invalidations {
            frame_interval_ms = Some(
                frame_interval_ms.map_or(invalidation.frame_interval_ms, |current| {
                    current.min(invalidation.frame_interval_ms)
                }),
            );
            if let Some(RetainedNodeUpdate::CompositingLayer(spec)) = invalidation.retained_update {
                if let Some(bounds) = tree.update_compositing_layer(&invalidation.target_id, spec) {
                    retained_dirty_bounds =
                        Some(retained_dirty_bounds.map_or(bounds, |current| current.union(bounds)));
                }
            } else if let Some(owner) = invalidation.owner {
                self.component_tree.mark_dirty(owner);
                regular_dirty_ids.push(invalidation.target_id.clone());
            } else {
                self.mark_component_owners(tree, std::iter::once(&invalidation.target_id));
                regular_dirty_ids.push(invalidation.target_id.clone());
            }
            route_changed |= self
                .component_states
                .take_route_invalidation(&invalidation.state_id);
        }
        self.dirty.mark_animation_ids(regular_dirty_ids);
        let dirty_bounds = match (self.dirty.take().bounds(tree), retained_dirty_bounds) {
            (Some(regular), Some(retained)) => Some(regular.union(retained)),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => None,
        };
        self.frame_interval_ms = frame_interval_ms;
        RuntimeOutput {
            events: Vec::new(),
            handler_events: Vec::new(),
            action_events: Vec::new(),
            default_actions: Vec::new(),
            dirty_bounds,
            animation_changed: animation_changed || component_changed,
            route_changed,
        }
    }

    pub fn sync_tree_animation_targets(&mut self, tree: &HostTree) -> bool {
        let mut changed = false;
        let interaction_events = self.events.sync_interaction_for_tree(tree);
        self.mark_component_owners(tree, interaction_events.iter().flat_map(event_target_ids));
        for event in interaction_events.iter().cloned() {
            self.dirty.mark_event(event);
        }
        changed |=
            apply_events_to_animations(tree, &mut self.animations, interaction_events.into_iter());
        changed |= self.animations.clear_absent_values_by(
            |id| tree.node(id).is_some(),
            &[
                AnimProperty::Hover,
                AnimProperty::Active,
                AnimProperty::Pressed,
                AnimProperty::Focus,
            ],
        );
        let sync_ids = tree.animation_sync_ids().cloned().collect::<Vec<_>>();
        for node in sync_ids.iter().filter_map(|id| tree.node(id)) {
            for (property, active) in node.animation_targets.iter().copied() {
                for binding in node
                    .animation_bindings
                    .iter()
                    .copied()
                    .filter(|binding| binding.property == property)
                {
                    if self
                        .animations
                        .sync_binding_target(node.id.clone(), binding, active)
                    {
                        if let Some(owner) = node.component_owner {
                            self.component_tree.mark_dirty(owner);
                        }
                        self.dirty.mark_id(node.id.clone());
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}
