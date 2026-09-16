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
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
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
            crate::core::content_text(value.to_string())
        })
    }
}

impl RootComponent for CountingRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
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
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
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
    let (mut tree, first) = mount_retained(&runtime, HostTree::new(), Arc::clone(&executions), 1);
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
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        component((), move |cx, _| {
            self.executions.fetch_add(1, Ordering::SeqCst);
            let state_id = cx.use_stable_id();
            *self.state_id.lock().expect("state ID poisoned") = Some(state_id.clone());
            crate::core::Element::new(move |cx| {
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
    state: Arc<Mutex<Option<crate::core::State<u32>>>>,
}

impl RootComponent for StatefulRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        component((), move |cx, _| {
            self.executions.fetch_add(1, Ordering::SeqCst);
            let state = cx.state(0_u32);
            let value = state.get();
            *self.state.lock().expect("state handle poisoned") = Some(state);
            crate::core::content_text(value.to_string())
        })
    }
}

fn mount_stateful(
    runtime: &UiRuntime,
    executions: Arc<AtomicUsize>,
    state: Arc<Mutex<Option<crate::core::State<u32>>>>,
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
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        let observed = Arc::clone(&self.observed);
        context_provider(
            self.value,
            component((), move |_cx, _| {
                observed
                    .lock()
                    .expect("context observation poisoned")
                    .push(crate::core::use_context::<u32>());
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
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
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
                    crate::core::content_text(*item)
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
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        if self.show_email {
            return component((), |_cx, _| {
                group(UiRect::new(0.0, 0.0, 30.0, 30.0)).child(crate::core::content_text("email"))
            })
            .key("email-form");
        }

        group(UiRect::new(0.0, 0.0, 30.0, 30.0)).content((
            crate::core::content_text("remembered"),
            crate::core::content_text("remembered details"),
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
