use super::{input::event_target_ids, *};

impl UiRuntime {
    pub fn sync_tree_focus(&mut self, tree: &HostTree) -> bool {
        if !tree.needs_focus_sync() {
            return false;
        }
        let events = self.events.sync_focus_for_tree(tree);
        if events.is_empty() {
            return false;
        }
        self.mark_component_owners(tree, events.iter().flat_map(event_target_ids));
        for event in &events {
            self.dirty.mark_event(event.clone());
        }
        apply_events_to_animations(tree, &mut self.animations, events);
        true
    }

    pub fn focus_node(&mut self, tree: &HostTree, id: &UiId) -> bool {
        !self.focus_node_events(tree, id).is_empty()
    }

    pub(super) fn focus_node_events(&mut self, tree: &HostTree, id: &UiId) -> Vec<UiEvent> {
        let events = self.events.focus_node(tree, id);
        if events.is_empty() {
            return events;
        }
        self.mark_component_owners(tree, events.iter().flat_map(event_target_ids));
        for event in &events {
            self.dirty.mark_event(event.clone());
        }
        apply_events_to_animations(tree, &mut self.animations, events.iter().cloned());
        events
    }
}
