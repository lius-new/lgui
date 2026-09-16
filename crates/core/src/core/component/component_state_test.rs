use super::super::ComponentTree;
use super::*;

#[derive(Clone, Default)]
struct TestState {
    value: u32,
}

impl ComponentState for TestState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Clone, Default)]
struct AnimatedTestState {
    value: u32,
}

impl ComponentState for AnimatedTestState {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn advance(&mut self, _elapsed_ms: f32) -> bool {
        self.value += 1;
        true
    }
}

fn component_owners() -> (ComponentId, ComponentId) {
    let components = ComponentTree::new();
    components.begin_render();
    let first = components.root(UiId::owned("first-owner"), "first-owner");
    let second = components.root(UiId::owned("second-owner"), "second-owner");
    components.end_render();
    (first, second)
}

#[test]
fn abort_frame_restores_existing_state_and_removes_new_state() {
    let store = ComponentStateStore::new();
    let existing = UiId::owned("existing");
    let created = UiId::owned("created");

    store.begin_frame();
    store.with_mut(&existing, |state: &mut TestState| state.value = 1);
    store.end_frame();

    store.begin_frame();
    store.with_mut(&existing, |state: &mut TestState| state.value = 2);
    store.with_mut(&created, |state: &mut TestState| state.value = 3);
    store.abort_frame();

    assert_eq!(
        store.with(&existing, |state: &TestState| state.value),
        Some(1)
    );
    assert!(!store.contains(&created));
}

#[test]
fn preserve_scope_keeps_reused_descendant_state_only() {
    let store = ComponentStateStore::new();
    let retained_scope = UiScope::new("ui").child("retained");
    let retained = retained_scope.id("h.0.0");
    let removed = UiScope::new("ui").child("removed").id("h.0.0");

    store.begin_frame();
    store.with_mut(&retained, |state: &mut TestState| state.value = 1);
    store.with_mut(&removed, |state: &mut TestState| state.value = 2);
    store.end_frame();

    store.begin_frame();
    store.preserve_scope(&retained_scope);
    store.end_frame();

    assert!(store.contains(&retained));
    assert!(!store.contains(&removed));
}

#[test]
fn bound_state_advance_targets_its_component_and_host_node() {
    let store = ComponentStateStore::new();
    let state_id = UiId::owned("rail.h.state.0");
    let target_id = UiId::owned("rail");
    let (owner, _) = component_owners();
    store.with_mut_for_component(
        &state_id,
        owner,
        target_id.clone(),
        |_state: &mut AnimatedTestState| {},
    );

    let invalidations = store.advance_invalidations(16.0);

    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0].state_id, state_id);
    assert_eq!(invalidations[0].target_id, target_id);
    assert_eq!(invalidations[0].owner, Some(owner));
}

#[test]
fn abort_frame_restores_state_binding_with_the_state_value() {
    let store = ComponentStateStore::new();
    let state_id = UiId::owned("rail.h.state.0");
    let first_target = UiId::owned("first-rail");
    let second_target = UiId::owned("second-rail");
    let (first_owner, second_owner) = component_owners();

    store.begin_frame();
    store.with_mut_for_component(
        &state_id,
        first_owner,
        first_target.clone(),
        |state: &mut AnimatedTestState| state.value = 7,
    );
    store.end_frame();

    store.begin_frame();
    store.with_mut_for_component(
        &state_id,
        second_owner,
        second_target,
        |state: &mut AnimatedTestState| state.value = 99,
    );
    store.abort_frame();

    assert_eq!(
        store.with(&state_id, |state: &AnimatedTestState| state.value),
        Some(7)
    );
    let invalidations = store.advance_invalidations(16.0);
    assert_eq!(invalidations[0].target_id, first_target);
    assert_eq!(invalidations[0].owner, Some(first_owner));
}

#[test]
fn unbound_state_advance_keeps_direct_node_invalidation() {
    let store = ComponentStateStore::new();
    let node_id = UiId::owned("slider");
    store.with_mut(&node_id, |_state: &mut AnimatedTestState| {});

    let invalidations = store.advance_invalidations(16.0);

    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0].state_id, node_id);
    assert_eq!(invalidations[0].target_id, node_id);
    assert_eq!(invalidations[0].owner, None);
}
