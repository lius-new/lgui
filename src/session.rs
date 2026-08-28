use std::sync::Arc;

use super::{
    application::AppView,
    core::{
        HostTree, HostTreeBuilder, LayoutCommitMetrics, LayoutRuntime, UiRuntime, UiScale,
        UiTaskSpawner, UiWake,
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

    pub fn render_view(&mut self, view: &AppView, viewport: UiRect, scale: UiScale) -> HostCommit {
        self.apply_pending_updates();
        self.prepare_render(viewport, scale);
        let mut tree =
            self.build_view_tree_from(self.render_tree(), Arc::clone(view), viewport, scale);
        if self.runtime.sync_tree_focus(&tree) {
            tree = self.build_view_tree_from(tree, Arc::clone(view), viewport, scale);
        }
        if self.runtime.sync_tree_animation_targets(&tree) {
            tree = self.build_view_tree_from(tree, Arc::clone(view), viewport, scale);
        }
        self.replace_tree(tree);
        self.commit(viewport)
    }

    fn build_view_tree_from(
        &self,
        retained: HostTree,
        view: AppView,
        viewport: UiRect,
        scale: UiScale,
    ) -> HostTree {
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
        builder.finish()
    }

    pub fn apply_pending_updates(&mut self) -> super::core::PendingUpdateOutput {
        let updates = self.runtime.apply_pending_updates();
        if updates.focus_changed {
            self.invalidate_all();
        }
        updates
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

    /// Releases retained drawing data while preserving state, hooks, effects and tasks.
    pub(crate) fn suspend_rendering(&mut self) {
        self.runtime.suspend_rendering();
        self.clear_host();
        self.invalidate_all();
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
    use crate::{
        application::AppView,
        core::{
            component, group, text, Color, Element, ElementRenderCx, InputEvent, InteractionRole,
            Point, PointerButton, State, TextStyle, UiElement, UiId, VisualStyle,
        },
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

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

    #[test]
    fn suspending_rendering_releases_the_host_and_preserves_component_state() {
        let mut session = UiSession::new();
        let components = session.runtime().component_tree();
        components.begin_render();
        let root = components.root(UiId::owned("session-root"), "session-root");
        components.begin_component_execution(root);
        components.finish_component(root);
        components.end_render();

        session.suspend_rendering();

        assert!(!session.has_tree());
        assert!(session.runtime().component_tree().is_alive(root));
        assert!(session.runtime().component_tree().is_dirty(root));
    }

    #[test]
    fn state_update_uses_host_diff_damage_instead_of_invalidating_the_component_bounds() {
        let state = Arc::new(Mutex::new(None::<State<i32>>));
        let captured = Arc::clone(&state);
        let view: AppView = Arc::new(move |cx| {
            let count = cx.state(0_i32);
            *captured.lock().expect("captured state poisoned") = Some(count.clone());
            group(UiRect::new(0, 0, 400, 300)).content((
                text(
                    UiRect::new(20, 20, 180, 60),
                    "unchanged",
                    TextStyle::new(Color::WHITE, 18, 400),
                ),
                text(
                    UiRect::new(20, 80, 180, 120),
                    format!("count={}", count.get()),
                    TextStyle::new(Color::WHITE, 18, 400),
                ),
            ))
        });
        let viewport = UiRect::new(0, 0, 400, 300);
        let mut session = UiSession::new();

        let first = session.render_view(&view, viewport, UiScale::ONE);
        assert!(first.damage.dirty.is_full());

        state
            .lock()
            .expect("captured state poisoned")
            .as_ref()
            .expect("state handle missing")
            .update(|count| *count += 1);
        let pending = session.apply_pending_updates();
        assert_eq!(pending.dirty_ids.len(), 1);

        let second = session.render_view(&view, viewport, UiScale::ONE);

        assert!(!second.damage.dirty.is_full());
        assert!(!second.damage.dirty.is_empty());
        assert!(second.damage.dirty.dirty_area() < second.damage.dirty.viewport_area() / 2);
        assert!(second
            .damage
            .dirty
            .effective_rects()
            .iter()
            .all(|rect| rect.intersect(UiRect::new(20, 20, 180, 60)).is_none()));
    }

    #[test]
    fn pointer_focus_reexecutes_components_losing_and_gaining_focus() {
        let first_executions = Arc::new(AtomicUsize::new(0));
        let second_executions = Arc::new(AtomicUsize::new(0));
        let first_observed_focus = Arc::new(Mutex::new(Vec::new()));
        let second_observed_focus = Arc::new(Mutex::new(Vec::new()));
        let first_id = Arc::new(Mutex::new(None::<UiId>));
        let second_id = Arc::new(Mutex::new(None::<UiId>));
        let view: AppView = Arc::new({
            let first_executions = Arc::clone(&first_executions);
            let second_executions = Arc::clone(&second_executions);
            let first_observed_focus = Arc::clone(&first_observed_focus);
            let second_observed_focus = Arc::clone(&second_observed_focus);
            let first_id = Arc::clone(&first_id);
            let second_id = Arc::clone(&second_id);
            move |_cx| {
                let field = |rect: UiRect,
                             executions: Arc<AtomicUsize>,
                             observed_focus: Arc<Mutex<Vec<bool>>>,
                             field_id: Arc<Mutex<Option<UiId>>>| {
                    component((), move |_cx, _props| {
                        executions.fetch_add(1, Ordering::SeqCst);
                        Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
                            let focused = cx.context.interaction_flags(&cx.id).focused;
                            observed_focus
                                .lock()
                                .expect("observed focus poisoned")
                                .push(focused);
                            *field_id.lock().expect("field id poisoned") = Some(cx.id.clone());
                            UiElement::panel(
                                cx.id,
                                rect,
                                VisualStyle::filled(if focused {
                                    Color(0x00FF00)
                                } else {
                                    Color(0xFF0000)
                                }),
                            )
                            .interaction(InteractionRole::Button)
                        })
                    })
                };
                group(UiRect::new(0, 0, 240, 120)).content((
                    field(
                        UiRect::new(0, 0, 120, 40),
                        Arc::clone(&first_executions),
                        Arc::clone(&first_observed_focus),
                        Arc::clone(&first_id),
                    ),
                    field(
                        UiRect::new(0, 60, 120, 100),
                        Arc::clone(&second_executions),
                        Arc::clone(&second_observed_focus),
                        Arc::clone(&second_id),
                    ),
                ))
            }
        });
        let viewport = UiRect::new(0, 0, 240, 120);
        let mut session = UiSession::new();

        session.render_view(&view, viewport, UiScale::ONE);
        let first_id = first_id
            .lock()
            .expect("first id poisoned")
            .clone()
            .expect("first id missing");
        let second_id = second_id
            .lock()
            .expect("second id poisoned")
            .clone()
            .expect("second id missing");

        click(&mut session, Point::new(10, 10));
        assert_eq!(
            session.runtime().interaction_state().focused,
            Some(first_id)
        );
        session.render_view(&view, viewport, UiScale::ONE);

        click(&mut session, Point::new(10, 70));
        assert_eq!(
            session.runtime().interaction_state().focused,
            Some(second_id)
        );
        session.render_view(&view, viewport, UiScale::ONE);

        assert_eq!(first_executions.load(Ordering::SeqCst), 3);
        assert_eq!(second_executions.load(Ordering::SeqCst), 2);
        assert_eq!(
            *first_observed_focus
                .lock()
                .expect("first observed focus poisoned"),
            vec![false, true, false]
        );
        assert_eq!(
            *second_observed_focus
                .lock()
                .expect("second observed focus poisoned"),
            vec![false, true]
        );
    }

    fn click(session: &mut UiSession, point: Point) {
        for input in [
            InputEvent::PointerDown {
                point,
                button: PointerButton::Left,
            },
            InputEvent::PointerUp {
                point,
                button: PointerButton::Left,
            },
        ] {
            let output = session.handle_input(input);
            if let Some(bounds) = output.dirty_bounds {
                session.invalidations_mut().invalidate_rect(bounds);
            }
        }
    }
}
