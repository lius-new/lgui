use super::{
    core::{
        HostTree, LayoutCommitMetrics, LayoutRuntime, UiRuntime, UiScale, UiTaskSpawner, UiWake,
    },
    frame::InvalidationSet,
    host::{HostCommit, HostRuntime},
};
use crate::core::UiRect;

/// Owns the complete retained UI state for one native window.
///
/// A backend may be recreated or switched without replacing this session, so component,
/// input, layout and scene identities survive renderer lifecycle changes.
#[derive(Default)]
pub struct UiSession {
    runtime: UiRuntime,
    host: HostRuntime,
    layout: LayoutRuntime,
    tree: HostTree,
    invalidations: InvalidationSet,
    render_context: Option<(UiRect, UiScale)>,
}

impl UiSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn runtime(&self) -> &UiRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut UiRuntime {
        &mut self.runtime
    }

    pub fn invalidations_mut(&mut self) -> &mut InvalidationSet {
        &mut self.invalidations
    }

    pub fn render_tree(&self) -> HostTree {
        self.tree.clone()
    }

    pub fn prepare_render(&mut self, viewport: UiRect, scale: UiScale) {
        let context = (viewport, scale);
        if self.render_context != Some(context) {
            self.runtime.invalidate_all_components();
            self.render_context = Some(context);
            self.invalidate_all();
        }
    }

    pub fn replace_tree(&mut self, tree: HostTree) {
        self.tree = tree;
    }

    pub fn tree(&self) -> &HostTree {
        &self.tree
    }

    pub fn has_tree(&self) -> bool {
        !self.tree.nodes().is_empty()
    }

    pub fn handle_input(&mut self, input: super::core::InputEvent) -> super::core::RuntimeOutput {
        self.runtime.handle_input(&self.tree, input)
    }

    pub fn advance(&mut self, elapsed_ms: f32) -> super::core::RuntimeOutput {
        self.runtime.advance(&self.tree, elapsed_ms)
    }

    pub fn commit(&mut self, viewport: UiRect) -> HostCommit {
        self.runtime.reconcile_tree(&self.tree);
        let changes = self.tree.take_projection_changes();
        let (_, changes) = self.layout.update_projection(&mut self.tree, changes);
        self.host.commit_projection(
            &self.tree,
            &self.runtime.interaction_state(),
            viewport,
            &mut self.invalidations,
            changes,
        )
    }

    pub fn apply_pending_updates(&mut self) {
        let updates = self.runtime.apply_pending_updates();
        for id in updates.dirty_ids {
            self.invalidations.invalidate_node(id);
        }
        if updates.focus_changed {
            self.invalidate_all();
        }
    }

    pub fn layout_metrics(&self) -> LayoutCommitMetrics {
        self.layout.metrics()
    }

    pub fn invalidate_all(&mut self) {
        self.runtime.invalidate_all_components();
        self.invalidations.invalidate_all();
    }

    pub fn clear_host(&mut self) {
        self.host.clear();
        self.layout.clear();
        self.tree = HostTree::new();
        self.render_context = None;
        self.invalidations = InvalidationSet::new();
    }

    pub fn reset(&mut self) {
        self.runtime.reset();
        self.clear_host();
    }

    pub fn set_wake(&self, wake: UiWake) {
        self.runtime.set_wake(wake);
    }

    pub fn set_task_spawner(&mut self, spawner: UiTaskSpawner) {
        self.runtime.set_task_spawner(spawner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::UiId;

    #[test]
    fn full_invalidation_marks_retained_components_dirty() {
        let mut session = UiSession::new();
        let components = session.runtime().component_tree();
        components.begin_render();
        let root = components.root(UiId::owned("session-root"), "session-root");
        components.begin_component_execution(root);
        components.finish_component(root);
        components.end_render();
        assert!(!components.is_dirty(root));

        session.invalidate_all();

        assert!(session.runtime().component_tree().is_dirty(root));
    }
}
