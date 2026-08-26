use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use super::{UiAction, UiId};

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

    fn take_route_invalidation(&mut self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct ComponentStateStore {
    states: RefCell<HashMap<UiId, Box<dyn ComponentState>>>,
    seen: RefCell<HashSet<UiId>>,
    rollback: RefCell<HashMap<UiId, Option<Box<dyn ComponentState>>>>,
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
        self.mark_seen(id);
        let mut states = self.states.borrow_mut();
        if self.tracking_frame.get() && !self.rollback.borrow().contains_key(id) {
            let previous = states.get(id).map(|state| {
                Box::new(
                    state
                        .as_any()
                        .downcast_ref::<T>()
                        .expect("component state type mismatch for UiId")
                        .clone(),
                ) as Box<dyn ComponentState>
            });
            self.rollback.borrow_mut().insert(id.clone(), previous);
        }
        let state = states
            .entry(id.clone())
            .or_insert_with(|| Box::<T>::default());
        let state = state
            .as_any_mut()
            .downcast_mut::<T>()
            .expect("component state type mismatch for UiId");
        f(state)
    }

    pub fn handle_action(&self, id: &UiId, action: &UiAction) -> ComponentActionOutcome {
        self.states
            .borrow_mut()
            .get_mut(id)
            .map_or_else(ComponentActionOutcome::ignored, |state| {
                state.handle_action(action)
            })
    }

    pub fn contains(&self, id: &UiId) -> bool {
        self.states.borrow().contains_key(id)
    }

    pub fn take_route_invalidation(&self, id: &UiId) -> bool {
        self.states
            .borrow_mut()
            .get_mut(id)
            .is_some_and(|state| state.take_route_invalidation())
    }

    pub fn advance(&self, elapsed_ms: f32) -> Vec<UiId> {
        let mut dirty = Vec::new();
        for (id, state) in self.states.borrow_mut().iter_mut() {
            if state.advance(elapsed_ms) {
                dirty.push(id.clone());
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
        let state = states.get(id)?.as_any().downcast_ref::<T>()?;
        Some(f(state))
    }

    pub fn preserve_scope(&self, scope: &UiId) {
        if !self.tracking_frame.get() {
            return;
        }
        let prefix = scope.as_str();
        let states = self.states.borrow();
        let mut seen = self.seen.borrow_mut();
        seen.extend(
            states
                .keys()
                .filter(|id| {
                    id.as_str() == prefix || id.as_str().starts_with(&format!("{prefix}."))
                })
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
}
