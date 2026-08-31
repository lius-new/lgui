use super::{
    AnimationRegistry, ComponentStateStore, ComponentTree, ContextRegistry, EffectRegistry,
    HookStateStore, HostTree, InteractionRole, LayoutSpec, RenderCx, RenderPhase, TextStyle,
    UiElement, UiId, UiInteractionState, UiNode, UiNodeKind, UiRect, UiRenderContext, UiScale,
    UiScope, UiTaskSpawner, UiUpdateQueue, VisualStyle,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub trait RootComponent {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> super::Element;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostProjectionMetrics {
    pub visited_nodes: usize,
    pub reused_component_roots: usize,
}

pub struct HostTreeBuilder {
    tree: HostTree,
    parent_stack: Vec<UiId>,
    retained: bool,
    fresh_owners: HashSet<super::ComponentId>,
    seen_by_owner: HashMap<super::ComponentId, HashSet<UiId>>,
    structure_changed: bool,
    projection_metrics: HostProjectionMetrics,
}

struct RenderTransaction<'a> {
    component_states: &'a ComponentStateStore,
    component_tree: &'a ComponentTree,
    contexts: &'a ContextRegistry,
    hook_states: &'a HookStateStore,
    effects: &'a EffectRegistry,
    committed: bool,
}

impl RenderTransaction<'_> {
    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for RenderTransaction<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.component_tree.abort_render();
        self.contexts.abort_render(self.component_tree);
        self.hook_states.abort_render(self.component_tree);
        self.component_states.abort_frame();
        self.effects.abort_frame();
    }
}

impl HostTreeBuilder {
    pub fn new() -> Self {
        Self {
            tree: HostTree::new(),
            parent_stack: Vec::new(),
            retained: false,
            fresh_owners: HashSet::new(),
            seen_by_owner: HashMap::new(),
            structure_changed: false,
            projection_metrics: HostProjectionMetrics::default(),
        }
    }

    pub fn from_retained(tree: HostTree) -> Self {
        Self {
            tree,
            parent_stack: Vec::new(),
            retained: true,
            fresh_owners: HashSet::new(),
            seen_by_owner: HashMap::new(),
            structure_changed: false,
            projection_metrics: HostProjectionMetrics::default(),
        }
    }

    pub fn push(&mut self, mut node: UiNode) -> UiId {
        if node.parent.is_none() {
            node.parent = self.parent_stack.last().cloned();
        }
        let id = node.id.clone();
        self.tree.push(node);
        id
    }

    pub(crate) fn mount_element(&mut self, element: UiElement) -> UiId {
        self.projection_metrics.visited_nodes += 1;
        let (mut node, children, boundary) = element.into_parts();
        if node.parent.is_none() {
            node.parent = self.parent_stack.last().cloned();
        }
        let id = node.id.clone();
        let previous_children = self
            .tree
            .node(&id)
            .map(|current| current.children.clone())
            .unwrap_or_default();
        let retain_children = boundary.is_some_and(|boundary| boundary.retain_children);
        if let Some(boundary) = boundary {
            if !boundary.retain_children {
                self.fresh_owners.insert(boundary.id);
            }
        }
        node.children = previous_children.clone();
        if retain_children {
            self.projection_metrics.reused_component_roots += 1;
        }
        if let Some(owner) = node.component_owner {
            self.seen_by_owner
                .entry(owner)
                .or_default()
                .insert(id.clone());
        }
        if self.retained {
            self.tree.upsert(node);
        } else {
            self.tree.push(node);
        }
        if !retain_children {
            self.parent_stack.push(id.clone());
            let next_children = children
                .iter()
                .cloned()
                .map(|child| self.mount_element(child))
                .collect::<Vec<_>>();
            self.parent_stack.pop();
            self.structure_changed |= previous_children != next_children;
            self.tree.set_children(&id, next_children);
        }
        id
    }

    pub fn node(&mut self, id: UiId, kind: UiNodeKind, rect: UiRect) -> UiId {
        self.push(UiNode::new(id, kind, rect))
    }

    pub fn interactive(
        &mut self,
        id: UiId,
        kind: UiNodeKind,
        rect: UiRect,
        interaction: InteractionRole,
    ) -> UiId {
        self.push(UiNode::new(id, kind, rect).interaction(interaction))
    }

    pub fn styled(&mut self, id: UiId, kind: UiNodeKind, rect: UiRect, style: VisualStyle) -> UiId {
        self.push(UiNode::new(id, kind, rect).style(style))
    }

    pub fn text(
        &mut self,
        id: UiId,
        rect: UiRect,
        text: impl Into<std::borrow::Cow<'static, str>>,
        style: TextStyle,
    ) -> UiId {
        self.push(UiNode::new(id, UiNodeKind::Text, rect).text(text, style))
    }

    pub fn laid_out(
        &mut self,
        id: UiId,
        kind: UiNodeKind,
        rect: UiRect,
        layout: LayoutSpec,
    ) -> UiId {
        self.push(UiNode::new(id, kind, rect).layout(layout))
    }

    pub fn in_phase(
        &mut self,
        id: UiId,
        kind: UiNodeKind,
        rect: UiRect,
        phase: RenderPhase,
    ) -> UiId {
        self.push(UiNode::new(id, kind, rect).render_phase(phase))
    }

    pub fn with_parent<T>(&mut self, id: UiId, build: impl FnOnce(&mut Self) -> T) -> T {
        self.parent_stack.push(id);
        let result = build(self);
        self.parent_stack.pop();
        result
    }

    pub fn element(&mut self, element: UiElement) -> UiId {
        element.mount(self)
    }

    pub fn mount(
        &mut self,
        component: impl RootComponent,
        viewport: UiRect,
        interaction: &UiInteractionState,
        animations: &AnimationRegistry,
        component_states: &ComponentStateStore,
        component_tree: &ComponentTree,
        contexts: &ContextRegistry,
        hook_states: &HookStateStore,
        hook_updates: &Arc<UiUpdateQueue>,
        task_spawner: Option<&UiTaskSpawner>,
        effects: &EffectRegistry,
        scale: UiScale,
    ) -> UiId {
        component_states.begin_frame();
        component_tree.begin_render();
        contexts.begin_render();
        hook_states.begin_render();
        effects.begin_frame();
        let mut transaction = RenderTransaction {
            component_states,
            component_tree,
            contexts,
            hook_states,
            effects,
            committed: false,
        };
        let scope = UiScope::new("ui");
        let context = UiRenderContext::new(
            interaction,
            animations,
            component_states,
            component_tree,
            contexts,
            hook_states,
            hook_updates,
            task_spawner,
            effects,
            viewport,
            scale,
        );
        let root = {
            let mut cx = RenderCx::new(&scope, &context);
            let _current_context = context.contexts().enter_current(cx.component_id());
            let view = component.render_root(&mut cx);
            let root = cx.component_id();
            self.element(
                cx.compile(view)
                    .claim_component_owner(root)
                    .component_boundary(root),
            )
        };
        component_states.end_frame();
        component_tree.end_render();
        self.fresh_owners
            .extend(component_tree.take_executed_in_render());
        for owner in self.fresh_owners.clone() {
            let keep = self.seen_by_owner.get(&owner).cloned().unwrap_or_default();
            self.structure_changed |= self.tree.retain_owner_nodes(owner, &keep);
        }
        self.structure_changed |= self.tree.prune_dead_component_owners(component_tree);
        if self.structure_changed {
            self.tree.reorder_by_hierarchy();
        }
        contexts.end_render(component_tree);
        hook_states.end_render(component_tree);
        effects.end_frame(component_tree);
        transaction.commit();
        root
    }

    pub fn finish(self) -> HostTree {
        self.tree
    }

    pub fn projection_metrics(&self) -> HostProjectionMetrics {
        self.projection_metrics
    }
}

impl Default for HostTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use std::{
        any::Any,
        panic::{catch_unwind, AssertUnwindSafe},
    };

    use super::*;
    use crate::core::{component, context_provider, group, ComponentState, UiRuntime};

    struct CountingRoot {
        executions: Arc<AtomicUsize>,
        props: u32,
    }

    struct TransactionRoot {
        value: u32,
        panic_during_render: bool,
        executions: Arc<AtomicUsize>,
        effect_runs: Arc<AtomicUsize>,
    }

    impl RootComponent for TransactionRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            let executions = Arc::clone(&self.executions);
            let effect_runs = Arc::clone(&self.effect_runs);
            let panic_during_render = self.panic_during_render;
            component(self.value, move |cx, value| {
                executions.fetch_add(1, Ordering::SeqCst);
                let effect_runs = Arc::clone(&effect_runs);
                cx.use_effect((*value,), move || {
                    effect_runs.fetch_add(1, Ordering::SeqCst);
                });
                if panic_during_render {
                    panic!("abandoned render");
                }
                super::super::content_text(value.to_string())
            })
        }
    }

    impl RootComponent for CountingRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            let executions = Arc::clone(&self.executions);
            component(self.props, move |_cx, _props| {
                executions.fetch_add(1, Ordering::SeqCst);
                group(UiRect::new(0.0, 0.0, 10.0, 10.0))
            })
        }
    }

    fn mount(runtime: &UiRuntime, executions: Arc<AtomicUsize>, props: u32) {
        let viewport = UiRect::new(0.0, 0.0, 10.0, 10.0);
        let interaction = runtime.interaction_state();
        let mut builder = HostTreeBuilder::new();
        builder.mount(
            CountingRoot { executions, props },
            viewport,
            &interaction,
            runtime.animations(),
            runtime.component_states(),
            runtime.component_tree(),
            runtime.contexts(),
            runtime.hook_states(),
            runtime.hook_updates(),
            runtime.task_spawner(),
            runtime.effects(),
            UiScale::ONE,
        );
    }

    fn mount_retained(
        runtime: &UiRuntime,
        tree: HostTree,
        executions: Arc<AtomicUsize>,
        props: u32,
    ) -> (HostTree, HostProjectionMetrics) {
        let viewport = UiRect::new(0.0, 0.0, 30.0, 10.0);
        let interaction = runtime.interaction_state();
        let mut builder = HostTreeBuilder::from_retained(tree);
        builder.mount(
            RetainedProjectionRoot { executions, props },
            viewport,
            &interaction,
            runtime.animations(),
            runtime.component_states(),
            runtime.component_tree(),
            runtime.contexts(),
            runtime.hook_states(),
            runtime.hook_updates(),
            runtime.task_spawner(),
            runtime.effects(),
            UiScale::ONE,
        );
        let metrics = builder.projection_metrics();
        (builder.finish(), metrics)
    }

    struct RetainedProjectionRoot {
        executions: Arc<AtomicUsize>,
        props: u32,
    }

    impl RootComponent for RetainedProjectionRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            let executions = Arc::clone(&self.executions);
            component(self.props, move |_cx, _| {
                executions.fetch_add(1, Ordering::SeqCst);
                group(UiRect::new(0.0, 0.0, 30.0, 10.0)).content((
                    group(UiRect::new(0.0, 0.0, 10.0, 10.0)),
                    group(UiRect::new(10.0, 0.0, 20.0, 10.0)),
                ))
            })
        }
    }

    #[test]
    fn declarative_component_skips_clean_execution_and_renders_changed_props() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(AtomicUsize::new(0));

        mount(&runtime, Arc::clone(&executions), 1);
        mount(&runtime, Arc::clone(&executions), 1);
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        mount(&runtime, Arc::clone(&executions), 2);
        assert_eq!(executions.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn clean_component_reuses_its_retained_host_subtree_without_visiting_descendants() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let (mut tree, first) =
            mount_retained(&runtime, HostTree::new(), Arc::clone(&executions), 1);
        assert_eq!(tree.nodes().len(), 3);
        assert_eq!(first.visited_nodes, 3);
        tree.take_projection_changes();

        let (mut tree, second) = mount_retained(&runtime, tree, Arc::clone(&executions), 1);
        let changes = tree.take_projection_changes();

        assert_eq!(tree.nodes().len(), 3);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(second.visited_nodes, 1);
        assert_eq!(second.reused_component_roots, 1);
        assert!(changes.changed.is_empty());
        assert!(changes.removed.is_empty());
        assert!(!changes.structure_changed);
    }

    #[test]
    fn identical_dirty_component_does_not_report_projection_changes() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let (mut tree, _) = mount_retained(&runtime, HostTree::new(), Arc::clone(&executions), 1);
        tree.take_projection_changes();
        runtime.component_tree().mark_all_dirty();

        let (mut tree, metrics) = mount_retained(&runtime, tree, Arc::clone(&executions), 1);
        let changes = tree.take_projection_changes();

        assert_eq!(executions.load(Ordering::SeqCst), 2);
        assert_eq!(metrics.visited_nodes, 3);
        assert!(changes.changed.is_empty());
        assert!(changes.removed.is_empty());
        assert!(!changes.structure_changed);
    }

    #[derive(Clone, Default)]
    struct RetainedLocalState {
        value: u32,
    }

    impl ComponentState for RetainedLocalState {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    struct ComponentStateRoot {
        executions: Arc<AtomicUsize>,
        state_id: Arc<Mutex<Option<UiId>>>,
    }

    impl RootComponent for ComponentStateRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            component((), move |cx, _| {
                self.executions.fetch_add(1, Ordering::SeqCst);
                let state_id = cx.use_stable_id();
                *self.state_id.lock().expect("state ID poisoned") = Some(state_id.clone());
                super::super::Element::new(move |cx| {
                    cx.context
                        .component_state_mut(&state_id, |state: &mut RetainedLocalState| {
                            state.value = 7;
                        });
                    UiElement::group(cx.id, UiRect::new(0.0, 0.0, 10.0, 10.0))
                })
            })
        }
    }

    fn mount_component_state_retained(
        runtime: &UiRuntime,
        tree: HostTree,
        executions: Arc<AtomicUsize>,
        state_id: Arc<Mutex<Option<UiId>>>,
    ) -> HostTree {
        let mut builder = HostTreeBuilder::from_retained(tree);
        builder.mount(
            ComponentStateRoot {
                executions,
                state_id,
            },
            UiRect::new(0.0, 0.0, 10.0, 10.0),
            &runtime.interaction_state(),
            runtime.animations(),
            runtime.component_states(),
            runtime.component_tree(),
            runtime.contexts(),
            runtime.hook_states(),
            runtime.hook_updates(),
            runtime.task_spawner(),
            runtime.effects(),
            UiScale::ONE,
        );
        builder.finish()
    }

    #[test]
    fn clean_component_reuse_preserves_component_state_in_its_element_scope() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let state_id = Arc::new(Mutex::new(None));
        let tree = mount_component_state_retained(
            &runtime,
            HostTree::new(),
            Arc::clone(&executions),
            Arc::clone(&state_id),
        );
        let retained = state_id
            .lock()
            .expect("state ID poisoned")
            .clone()
            .expect("state ID missing");
        assert!(runtime.component_states().contains(&retained));

        let _tree = mount_component_state_retained(
            &runtime,
            tree,
            Arc::clone(&executions),
            Arc::clone(&state_id),
        );

        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(runtime.component_states().contains(&retained));
    }

    struct StatefulRoot {
        executions: Arc<AtomicUsize>,
        state: Arc<Mutex<Option<super::super::State<u32>>>>,
    }

    impl RootComponent for StatefulRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            component((), move |cx, _| {
                self.executions.fetch_add(1, Ordering::SeqCst);
                let state = cx.state(0_u32);
                let value = state.get();
                *self.state.lock().expect("state handle poisoned") = Some(state);
                super::super::content_text(value.to_string())
            })
        }
    }

    fn mount_stateful(
        runtime: &UiRuntime,
        executions: Arc<AtomicUsize>,
        state: Arc<Mutex<Option<super::super::State<u32>>>>,
    ) {
        let viewport = UiRect::new(0.0, 0.0, 10.0, 10.0);
        let interaction = runtime.interaction_state();
        let mut builder = HostTreeBuilder::new();
        builder.mount(
            StatefulRoot { executions, state },
            viewport,
            &interaction,
            runtime.animations(),
            runtime.component_states(),
            runtime.component_tree(),
            runtime.contexts(),
            runtime.hook_states(),
            runtime.hook_updates(),
            runtime.task_spawner(),
            runtime.effects(),
            UiScale::ONE,
        );
    }

    #[test]
    fn state_update_marks_only_its_declarative_component_for_execution() {
        let mut runtime = UiRuntime::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let state = Arc::new(Mutex::new(None));
        mount_stateful(&runtime, Arc::clone(&executions), Arc::clone(&state));
        mount_stateful(&runtime, Arc::clone(&executions), Arc::clone(&state));
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        let count = state
            .lock()
            .expect("state handle poisoned")
            .clone()
            .expect("state handle missing");
        count.update(|value| *value += 3);
        count.update(|value| *value += 4);
        assert_eq!(count.get(), 7);
        assert_eq!(
            runtime
                .apply_pending_updates(&HostTree::new())
                .dirty_ids
                .len(),
            1
        );
        mount_stateful(&runtime, Arc::clone(&executions), Arc::clone(&state));

        assert_eq!(executions.load(Ordering::SeqCst), 2);
    }

    struct ContextRoot {
        value: u32,
        observed: Arc<Mutex<Vec<u32>>>,
    }

    impl RootComponent for ContextRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            let observed = Arc::clone(&self.observed);
            context_provider(
                self.value,
                component((), move |_cx, _| {
                    observed
                        .lock()
                        .expect("context observation poisoned")
                        .push(super::super::use_context::<u32>());
                    group(UiRect::new(0.0, 0.0, 10.0, 10.0))
                }),
            )
        }
    }

    #[test]
    fn declarative_context_hook_reads_from_deferred_descendants_and_tracks_changes() {
        let runtime = UiRuntime::new();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let viewport = UiRect::new(0.0, 0.0, 10.0, 10.0);
        let interaction = runtime.interaction_state();
        for value in [7, 7, 9] {
            let mut builder = HostTreeBuilder::new();
            builder.mount(
                ContextRoot {
                    value,
                    observed: Arc::clone(&observed),
                },
                viewport,
                &interaction,
                runtime.animations(),
                runtime.component_states(),
                runtime.component_tree(),
                runtime.contexts(),
                runtime.hook_states(),
                runtime.hook_updates(),
                runtime.task_spawner(),
                runtime.effects(),
                UiScale::ONE,
            );
        }

        assert_eq!(
            &*observed.lock().expect("context observation poisoned"),
            &[7, 9]
        );
    }

    struct KeyedListRoot {
        items: Vec<&'static str>,
        executions: Arc<Mutex<HashMap<&'static str, usize>>>,
    }

    impl RootComponent for KeyedListRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            let rows = self
                .items
                .into_iter()
                .map(|item| {
                    let executions = Arc::clone(&self.executions);
                    component(item, move |_cx, item| {
                        *executions
                            .lock()
                            .expect("keyed execution counter poisoned")
                            .entry(*item)
                            .or_default() += 1;
                        super::super::content_text(*item)
                    })
                    .key(item)
                })
                .collect::<Vec<_>>();
            group(UiRect::new(0.0, 0.0, 30.0, 30.0)).content(rows)
        }
    }

    fn mount_keyed_list(
        runtime: &UiRuntime,
        tree: HostTree,
        items: Vec<&'static str>,
        executions: Arc<Mutex<HashMap<&'static str, usize>>>,
    ) -> HostTree {
        let viewport = UiRect::new(0.0, 0.0, 30.0, 30.0);
        let mut builder = HostTreeBuilder::from_retained(tree);
        builder.mount(
            KeyedListRoot { items, executions },
            viewport,
            &runtime.interaction_state(),
            runtime.animations(),
            runtime.component_states(),
            runtime.component_tree(),
            runtime.contexts(),
            runtime.hook_states(),
            runtime.hook_updates(),
            runtime.task_spawner(),
            runtime.effects(),
            UiScale::ONE,
        );
        builder.finish()
    }

    #[test]
    fn keyed_reorder_reuses_components_and_reorders_retained_host_children() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(Mutex::new(HashMap::new()));
        let tree = mount_keyed_list(
            &runtime,
            HostTree::new(),
            vec!["alpha", "beta"],
            Arc::clone(&executions),
        );
        let tree = mount_keyed_list(
            &runtime,
            tree,
            vec!["beta", "alpha"],
            Arc::clone(&executions),
        );

        let labels = tree
            .nodes()
            .iter()
            .filter_map(|node| node.text.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["beta", "alpha"]);
        assert_eq!(
            *executions.lock().expect("keyed execution counter poisoned"),
            HashMap::from([("alpha", 1), ("beta", 1)])
        );
    }

    #[test]
    fn removing_a_keyed_component_prunes_its_retained_host_subtree() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(Mutex::new(HashMap::new()));
        let tree = mount_keyed_list(
            &runtime,
            HostTree::new(),
            vec!["alpha", "beta"],
            Arc::clone(&executions),
        );
        let tree = mount_keyed_list(&runtime, tree, vec!["alpha"], executions);

        let labels = tree
            .nodes()
            .iter()
            .filter_map(|node| node.text.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["alpha"]);
        assert_eq!(tree.nodes().len(), 2);
        assert_eq!(runtime.component_tree().metrics().unmounted, 1);
    }

    struct RootBranchReplacement {
        show_email: bool,
    }

    impl RootComponent for RootBranchReplacement {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> super::super::Element {
            if self.show_email {
                return component((), |_cx, _| {
                    group(UiRect::new(0.0, 0.0, 30.0, 30.0))
                        .child(super::super::content_text("email"))
                })
                .key("email-form");
            }

            group(UiRect::new(0.0, 0.0, 30.0, 30.0)).content((
                super::super::content_text("remembered"),
                super::super::content_text("remembered details"),
            ))
        }
    }

    fn mount_root_branch(runtime: &UiRuntime, tree: HostTree, show_email: bool) -> HostTree {
        let mut builder = HostTreeBuilder::from_retained(tree);
        builder.mount(
            RootBranchReplacement { show_email },
            UiRect::new(0.0, 0.0, 30.0, 30.0),
            &runtime.interaction_state(),
            runtime.animations(),
            runtime.component_states(),
            runtime.component_tree(),
            runtime.contexts(),
            runtime.hook_states(),
            runtime.hook_updates(),
            runtime.task_spawner(),
            runtime.effects(),
            UiScale::ONE,
        );
        builder.finish()
    }

    #[test]
    fn replacing_a_plain_component_root_with_a_child_component_prunes_the_old_host_subtree() {
        let runtime = UiRuntime::new();
        let mut tree = mount_root_branch(&runtime, HostTree::new(), false);
        tree.take_projection_changes();
        let tree = mount_root_branch(&runtime, tree, true);

        let labels = tree
            .nodes()
            .iter()
            .filter_map(|node| node.text.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["email"]);
    }

    #[test]
    fn panicking_render_keeps_retained_tree_and_does_not_run_staged_effects() {
        let runtime = UiRuntime::new();
        let executions = Arc::new(AtomicUsize::new(0));
        let effect_runs = Arc::new(AtomicUsize::new(0));
        let viewport = UiRect::new(0.0, 0.0, 30.0, 10.0);
        let mount = |builder: &mut HostTreeBuilder, value, panic_during_render| {
            builder.mount(
                TransactionRoot {
                    value,
                    panic_during_render,
                    executions: Arc::clone(&executions),
                    effect_runs: Arc::clone(&effect_runs),
                },
                viewport,
                &runtime.interaction_state(),
                runtime.animations(),
                runtime.component_states(),
                runtime.component_tree(),
                runtime.contexts(),
                runtime.hook_states(),
                runtime.hook_updates(),
                runtime.task_spawner(),
                runtime.effects(),
                UiScale::ONE,
            );
        };

        let mut first = HostTreeBuilder::new();
        mount(&mut first, 1, false);
        let committed = first.finish();
        runtime.run_effects();
        assert_eq!(effect_runs.load(Ordering::SeqCst), 1);

        let expected_nodes = committed
            .nodes()
            .iter()
            .map(|node| (node.id.clone(), node.text.clone()))
            .collect::<Vec<_>>();
        let mut abandoned = HostTreeBuilder::from_retained(committed);
        let result = catch_unwind(AssertUnwindSafe(|| mount(&mut abandoned, 2, true)));
        assert!(result.is_err());
        runtime.run_effects();

        assert_eq!(effect_runs.load(Ordering::SeqCst), 1);
        assert_eq!(
            abandoned
                .tree
                .nodes()
                .iter()
                .map(|node| (node.id.clone(), node.text.clone()))
                .collect::<Vec<_>>(),
            expected_nodes
        );

        mount(&mut abandoned, 1, false);
        assert_eq!(executions.load(Ordering::SeqCst), 2);
        runtime.run_effects();
        assert_eq!(effect_runs.load(Ordering::SeqCst), 1);
    }
}
