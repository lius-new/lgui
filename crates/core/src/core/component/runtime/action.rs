use super::{
    input::{event_target_ids, interaction_state_target_ids},
    *,
};

impl UiRuntime {
    pub fn handle_default_action(
        &mut self,
        tree: &HostTree,
        pending: UiDefaultAction,
    ) -> RuntimeOutput {
        if pending.action.id().as_str() == FOCUS_TRAVERSAL_ACTION {
            let events = self
                .events
                .focus_adjacent(tree, pending.action.payload_value() == Some("reverse"));
            let handler_events = events
                .iter()
                .flat_map(|event| tree.handler_events(event))
                .collect();
            for event in events.iter().cloned() {
                self.dirty.mark_event(event);
            }
            self.mark_component_owners(tree, events.iter().flat_map(interaction_state_target_ids));
            let animation_changed =
                apply_events_to_animations(tree, &mut self.animations, events.iter().cloned());
            if animation_changed {
                self.mark_component_owners(tree, events.iter().flat_map(event_target_ids));
            }
            let dirty_bounds = self.dirty.take().bounds(tree);
            return RuntimeOutput {
                events,
                handler_events,
                action_events: Vec::new(),
                default_actions: Vec::new(),
                dirty_bounds,
                animation_changed,
                route_changed: false,
            };
        }
        let mut action_events = Vec::new();
        let (_, changed, route_changed) = self.apply_component_action(
            tree,
            &pending.action_target,
            &pending.action,
            &mut action_events,
        );
        let handler_events = changed
            .then(|| {
                tree.handler_event(
                    &pending.action_target,
                    UiEventPayload::Change {
                        value: pending.action.payload_value().map(str::to_owned),
                    },
                )
            })
            .flatten()
            .into_iter()
            .collect();
        let dirty_bounds = self.dirty.take().bounds(tree);
        RuntimeOutput {
            events: Vec::new(),
            handler_events,
            action_events,
            default_actions: Vec::new(),
            dirty_bounds,
            animation_changed: changed,
            route_changed,
        }
    }

    fn apply_component_action(
        &mut self,
        tree: &HostTree,
        target: &UiId,
        action: &UiAction,
        action_events: &mut Vec<UiActionEvent>,
    ) -> (bool, bool, bool) {
        let outcome = self.component_states.handle_action(target, action);
        if outcome.changed {
            self.mark_component_owners(tree, std::iter::once(target));
            self.dirty.mark_id(target.clone());
        }
        for action in outcome.events {
            let Some(handler) = tree.action_handler(target, action.id()) else {
                continue;
            };
            action_events.push(UiActionEvent {
                target: target.clone(),
                action,
                handler,
            });
        }
        let route_changed =
            outcome.changed && self.component_states.take_route_invalidation(target);
        (outcome.handled, outcome.changed, route_changed)
    }
}
