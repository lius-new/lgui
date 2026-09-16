use std::sync::Arc;
#[cfg(feature = "diagnostics-timing")]
use std::time::Instant;

use super::{
    frame::InvalidationSet,
    host::{HostCommit, HostRuntime},
};
use crate::core::UiRect;
use crate::{
    application::AppView,
    core::{
        ComponentRuntimeMetrics, HostProjectionMetrics, HostTree, HostTreeBuilder,
        LayoutCommitMetrics, LayoutRuntime, UiRuntime, UiScale, UiTaskSpawner, UiWake,
    },
};

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
    component_metrics: ComponentRuntimeMetrics,
    projection_metrics: HostProjectionMetrics,
    #[cfg(feature = "diagnostics-timing")]
    render_timings: SessionRenderTimings,
}

#[cfg(feature = "diagnostics-timing")]
#[derive(Clone, Copy, Debug, Default)]
#[doc(hidden)]
pub struct SessionRenderTimings {
    pub pending_updates_ms: f32,
    pub prepare_render_ms: f32,
    pub retained_snapshot_ms: f32,
    pub declarative_mount_ms: f32,
    pub focus_animation_sync_ms: f32,
    pub focus_sync_ms: f32,
    pub focus_rebuild_ms: f32,
    pub animation_target_sync_ms: f32,
    pub animation_rebuild_ms: f32,
    pub animation_sync_nodes: usize,
    pub focus_sync_needed: bool,
    pub runtime_reconcile_ms: f32,
    pub layout_ms: f32,
    pub host_commit_ms: f32,
    pub total_ms: f32,
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

    pub fn handle_input(&mut self, input: crate::core::InputEvent) -> crate::core::RuntimeOutput {
        self.runtime.handle_input(&self.tree, input)
    }

    pub fn advance(&mut self, elapsed_ms: f32) -> crate::core::RuntimeOutput {
        self.runtime.advance(&mut self.tree, elapsed_ms)
    }

    pub fn handle_default_action(
        &mut self,
        action: crate::core::UiDefaultAction,
    ) -> crate::core::RuntimeOutput {
        self.runtime.handle_default_action(&self.tree, action)
    }

    pub fn commit(&mut self, viewport: UiRect) -> HostCommit {
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

    pub fn render_view(&mut self, view: &AppView, viewport: UiRect, scale: UiScale) -> HostCommit {
        #[cfg(feature = "diagnostics-timing")]
        let total_started = Instant::now();
        #[cfg(feature = "diagnostics-timing")]
        let pending_updates_started = Instant::now();
        self.apply_pending_updates();
        #[cfg(feature = "diagnostics-timing")]
        let pending_updates_ms = elapsed_ms(pending_updates_started);

        #[cfg(feature = "diagnostics-timing")]
        let prepare_render_started = Instant::now();
        self.prepare_render(viewport, scale);
        #[cfg(feature = "diagnostics-timing")]
        let prepare_render_ms = elapsed_ms(prepare_render_started);

        #[cfg(feature = "diagnostics-timing")]
        let retained_snapshot_started = Instant::now();
        let retained = self.render_tree();
        #[cfg(feature = "diagnostics-timing")]
        let retained_snapshot_ms = elapsed_ms(retained_snapshot_started);

        #[cfg(feature = "diagnostics-timing")]
        let declarative_mount_started = Instant::now();
        let (mut tree, mut projection_metrics) =
            self.build_view_tree_from(retained, Arc::clone(view), viewport, scale);
        #[cfg(feature = "diagnostics-timing")]
        let declarative_mount_ms = elapsed_ms(declarative_mount_started);

        #[cfg(feature = "diagnostics-timing")]
        let focus_sync_started = Instant::now();
        #[cfg(feature = "diagnostics-timing")]
        let focus_sync_needed = tree.needs_focus_sync();
        let focus_changed = self.runtime.sync_tree_focus(&tree);
        #[cfg(feature = "diagnostics-timing")]
        let focus_sync_ms = elapsed_ms(focus_sync_started);
        #[cfg(feature = "diagnostics-timing")]
        let focus_rebuild_started = Instant::now();
        if focus_changed {
            (tree, projection_metrics) =
                self.build_view_tree_from(tree, Arc::clone(view), viewport, scale);
        }
        #[cfg(feature = "diagnostics-timing")]
        let focus_rebuild_ms = elapsed_ms(focus_rebuild_started);
        #[cfg(feature = "diagnostics-timing")]
        let animation_target_sync_started = Instant::now();
        #[cfg(feature = "diagnostics-timing")]
        let animation_sync_nodes = tree.animation_sync_ids().count();
        let animation_targets_changed = self.runtime.sync_tree_animation_targets(&tree);
        #[cfg(feature = "diagnostics-timing")]
        let animation_target_sync_ms = elapsed_ms(animation_target_sync_started);
        #[cfg(feature = "diagnostics-timing")]
        let animation_rebuild_started = Instant::now();
        if animation_targets_changed {
            (tree, projection_metrics) =
                self.build_view_tree_from(tree, Arc::clone(view), viewport, scale);
        }
        #[cfg(feature = "diagnostics-timing")]
        let animation_rebuild_ms = elapsed_ms(animation_rebuild_started);
        self.component_metrics = self.runtime.component_tree().metrics();
        self.projection_metrics = projection_metrics;
        self.replace_tree(tree);
        #[cfg(feature = "diagnostics-timing")]
        let focus_animation_sync_ms =
            focus_sync_ms + focus_rebuild_ms + animation_target_sync_ms + animation_rebuild_ms;

        #[cfg(feature = "diagnostics-timing")]
        let layout_started = Instant::now();
        let changes = self.tree.take_projection_changes();
        let (_, changes) = self.layout.update_projection(&mut self.tree, changes);
        #[cfg(feature = "diagnostics-timing")]
        let layout_ms = elapsed_ms(layout_started);

        #[cfg(feature = "diagnostics-timing")]
        let host_commit_started = Instant::now();
        let commit = self.host.commit_projection(
            &self.tree,
            &self.runtime.interaction_state(),
            viewport,
            &mut self.invalidations,
            changes,
        );
        #[cfg(feature = "diagnostics-timing")]
        {
            self.render_timings = SessionRenderTimings {
                pending_updates_ms,
                prepare_render_ms,
                retained_snapshot_ms,
                declarative_mount_ms,
                focus_animation_sync_ms,
                focus_sync_ms,
                focus_rebuild_ms,
                animation_target_sync_ms,
                animation_rebuild_ms,
                animation_sync_nodes,
                focus_sync_needed,
                runtime_reconcile_ms: 0.0,
                layout_ms,
                host_commit_ms: elapsed_ms(host_commit_started),
                total_ms: elapsed_ms(total_started),
            };
        }
        commit
    }

    fn build_view_tree_from(
        &self,
        retained: HostTree,
        view: AppView,
        viewport: UiRect,
        scale: UiScale,
    ) -> (HostTree, HostProjectionMetrics) {
        let interaction = self.runtime.interaction_state();
        let mut builder = HostTreeBuilder::from_retained(retained);
        builder.mount(
            view,
            viewport,
            &interaction,
            self.runtime.animations(),
            self.runtime.component_states(),
            self.runtime.component_tree(),
            self.runtime.contexts(),
            self.runtime.hook_states(),
            self.runtime.hook_updates(),
            self.runtime.task_spawner(),
            self.runtime.effects(),
            scale,
        );
        let metrics = builder.projection_metrics();
        (builder.finish(), metrics)
    }

    pub fn apply_pending_updates(&mut self) -> crate::core::PendingUpdateOutput {
        let updates = self.runtime.apply_pending_updates(&self.tree);
        if updates.focus_changed {
            self.invalidate_all();
        }
        updates
    }

    pub fn layout_metrics(&self) -> LayoutCommitMetrics {
        self.layout.metrics()
    }

    pub fn component_metrics(&self) -> ComponentRuntimeMetrics {
        self.component_metrics
    }

    pub fn projection_metrics(&self) -> HostProjectionMetrics {
        self.projection_metrics
    }

    pub(crate) fn memory_usage(&self) -> (crate::memory::CacheUsage, crate::memory::CacheUsage) {
        let component = self.runtime.component_tree().output_memory_usage();
        let host_scene_bytes = self
            .tree
            .estimated_bytes()
            .saturating_add(self.host.estimated_bytes());
        let host_scene = crate::memory::CacheUsage {
            rebuildable_bytes: host_scene_bytes,
            cpu_bytes: host_scene_bytes,
            entries: self
                .tree
                .nodes()
                .len()
                .saturating_add(usize::from(self.has_tree())),
            largest_entry_bytes: self.host.estimated_bytes(),
            ..Default::default()
        };
        (component, host_scene)
    }

    pub(crate) fn trim_component_outputs(&mut self, target_bytes: usize) -> usize {
        let released = self.runtime.component_tree().trim_outputs(target_bytes);
        if released > 0 {
            self.invalidations.invalidate_all();
        }
        released
    }

    pub(crate) fn trim_host_scene(&mut self) -> usize {
        let before = self.memory_usage().1.rebuildable_bytes;
        self.clear_host();
        self.invalidate_all();
        before
    }

    #[cfg(feature = "diagnostics-timing")]
    #[doc(hidden)]
    pub fn render_timings(&self) -> SessionRenderTimings {
        self.render_timings
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

    /// Releases retained drawing data while preserving state, hooks, effects and tasks.
    pub(crate) fn suspend_rendering(&mut self) {
        self.runtime.suspend_rendering();
        self.trim_component_outputs(0);
        self.trim_host_scene();
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

#[cfg(feature = "diagnostics-timing")]
fn elapsed_ms(started: Instant) -> f32 {
    started.elapsed().as_secs_f32() * 1_000.0
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
