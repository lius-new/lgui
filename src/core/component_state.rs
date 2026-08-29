use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use super::{ComponentId, UiAction, UiId, UiScope};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentActionOutcome {
    pub handled: bool,
    pub changed: bool,
    pub events: Vec<UiAction>,
}

impl ComponentActionOutcome {
    pub const fn ignored() -> Self {
        Self {
            handled: false,
            changed: false,
            events: Vec::new(),
        }
    }

    pub const fn handled(changed: bool) -> Self {
        Self {
            handled: true,
            changed,
            events: Vec::new(),
        }
    }

    pub fn emit(mut self, event: UiAction) -> Self {
        self.events.push(event);
        self
    }
}

impl From<bool> for ComponentActionOutcome {
    fn from(changed: bool) -> Self {
        if changed {
            Self::handled(true)
        } else {
            Self::ignored()
        }
    }
}

pub trait ComponentState: Any {
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn handle_action(&mut self, _action: &UiAction) -> ComponentActionOutcome {
        ComponentActionOutcome::ignored()
    }

    fn advance(&mut self, _elapsed_ms: f32) -> bool {
        false
    }

    fn wants_frame(&self) -> bool {
        false
    }

    fn frame_interval_ms(&self) -> u64 {
        16
    }

    fn take_route_invalidation(&mut self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComponentStateBinding {
    pub owner: ComponentId,
    pub invalidation_id: UiId,
}

pub(crate) struct ComponentStateInvalidation {
    pub state_id: UiId,
    pub target_id: UiId,
    pub owner: Option<ComponentId>,
    pub frame_interval_ms: u64,
}

struct ComponentStateEntry {
    state: Box<dyn ComponentState>,
    binding: Option<ComponentStateBinding>,
}

#[derive(Default)]
pub struct ComponentStateStore {
    states: RefCell<HashMap<UiId, ComponentStateEntry>>,
    seen: RefCell<HashSet<UiId>>,
    rollback: RefCell<HashMap<UiId, Option<ComponentStateEntry>>>,
    tracking_frame: Cell<bool>,
}

impl ComponentStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_frame(&self) {
        if self.tracking_frame.get() {
            self.abort_frame();
        }
        self.seen.borrow_mut().clear();
        self.rollback.borrow_mut().clear();
        self.tracking_frame.set(true);
    }

    pub fn end_frame(&self) {
        let seen = self.seen.borrow();
        self.states.borrow_mut().retain(|id, _| seen.contains(id));
        self.tracking_frame.set(false);
        drop(seen);
        self.seen.borrow_mut().clear();
        self.rollback.borrow_mut().clear();
    }

    pub fn abort_frame(&self) {
        let rollback = std::mem::take(&mut *self.rollback.borrow_mut());
        let mut states = self.states.borrow_mut();
        for (id, previous) in rollback {
            match previous {
                Some(previous) => {
                    states.insert(id, previous);
                }
                None => {
                    states.remove(&id);
                }
            }
        }
        self.tracking_frame.set(false);
        self.seen.borrow_mut().clear();
    }

    pub fn with_mut<T, R>(&self, id: &UiId, f: impl FnOnce(&mut T) -> R) -> R
    where
        T: ComponentState + Clone + Default + 'static,
    {
        self.with_mut_inner(id, None, f)
    }

    pub fn with_mut_for_component<T, R>(
        &self,
        id: &UiId,
        owner: ComponentId,
        invalidation_id: UiId,
        f: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: ComponentState + Clone + Default + 'static,
    {
        self.with_mut_inner(
            id,
            Some(ComponentStateBinding {
                owner,
                invalidation_id,
            }),
            f,
        )
    }

    fn with_mut_inner<T, R>(
        &self,
        id: &UiId,
        binding: Option<ComponentStateBinding>,
        f: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: ComponentState + Clone + Default + 'static,
    {
        self.mark_seen(id);
        let mut states = self.states.borrow_mut();
        if self.tracking_frame.get() && !self.rollback.borrow().contains_key(id) {
            let previous = states.get(id).map(|entry| ComponentStateEntry {
                state: Box::new(
                    entry
                        .state
                        .as_any()
                        .downcast_ref::<T>()
                        .expect("component state type mismatch for UiId")
                        .clone(),
                ) as Box<dyn ComponentState>,
                binding: entry.binding.clone(),
            });
            self.rollback.borrow_mut().insert(id.clone(), previous);
        }
        let entry = states
            .entry(id.clone())
            .or_insert_with(|| ComponentStateEntry {
                state: Box::<T>::default(),
                binding: None,
            });
        entry.binding = binding;
        let state = entry
            .state
            .as_any_mut()
            .downcast_mut::<T>()
            .expect("component state type mismatch for UiId");
        f(state)
    }

    pub fn handle_action(&self, id: &UiId, action: &UiAction) -> ComponentActionOutcome {
        self.states
            .borrow_mut()
            .get_mut(id)
            .map_or_else(ComponentActionOutcome::ignored, |entry| {
                entry.state.handle_action(action)
            })
    }

    pub fn contains(&self, id: &UiId) -> bool {
        self.states.borrow().contains_key(id)
    }

    pub fn take_route_invalidation(&self, id: &UiId) -> bool {
        self.states
            .borrow_mut()
            .get_mut(id)
            .is_some_and(|entry| entry.state.take_route_invalidation())
    }

    pub fn advance(&self, elapsed_ms: f32) -> Vec<UiId> {
        self.advance_invalidations(elapsed_ms)
            .into_iter()
            .map(|invalidation| invalidation.state_id)
            .collect()
    }

    pub(crate) fn advance_invalidations(&self, elapsed_ms: f32) -> Vec<ComponentStateInvalidation> {
        let mut dirty = Vec::new();
        for (id, entry) in self.states.borrow_mut().iter_mut() {
            if entry.state.advance(elapsed_ms) {
                dirty.push(ComponentStateInvalidation {
                    state_id: id.clone(),
                    target_id: entry
                        .binding
                        .as_ref()
                        .map_or_else(|| id.clone(), |binding| binding.invalidation_id.clone()),
                    owner: entry.binding.as_ref().map(|binding| binding.owner),
                    frame_interval_ms: entry.state.frame_interval_ms().max(1),
                });
            }
        }
        dirty
    }

    #[cfg(test)]
    pub fn with<T, R>(&self, id: &UiId, f: impl FnOnce(&T) -> R) -> Option<R>
    where
        T: ComponentState + 'static,
    {
        self.mark_seen(id);
        let states = self.states.borrow();
        let state = states.get(id)?.state.as_any().downcast_ref::<T>()?;
        Some(f(state))
    }

    pub fn preserve_scope(&self, scope: &UiScope) {
        if !self.tracking_frame.get() {
            return;
        }
        let scope_id = scope.scope_id();
        let prefix = scope_id.as_str();
        let nested_prefix = format!("{prefix}.");
        let states = self.states.borrow();
        let mut seen = self.seen.borrow_mut();
        seen.extend(
            states
                .keys()
                .filter(|id| id.as_str() == prefix || id.as_str().starts_with(&nested_prefix))
                .cloned(),
        );
    }

    fn mark_seen(&self, id: &UiId) {
        if self.tracking_frame.get() {
            self.seen.borrow_mut().insert(id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
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
}
