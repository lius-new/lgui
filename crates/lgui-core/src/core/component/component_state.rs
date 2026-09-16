use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use super::{ComponentId, CompositingLayerSpec, UiAction, UiId, UiScope};

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

/// Application-owned animation state for a retained compositing layer.
///
/// The framework advances this state and applies the resulting composition properties directly
/// to the retained node. Static layer children are not rebuilt when only the spec changes.
pub trait CompositingLayerAnimation: Clone + Default + 'static {
    fn advance(&mut self, elapsed_ms: f32) -> bool;

    fn compositing_layer_spec(&self) -> CompositingLayerSpec;

    fn wants_frame(&self) -> bool {
        true
    }

    fn frame_interval_ms(&self) -> u64 {
        16
    }
}

#[derive(Clone, Default)]
struct CompositingLayerAnimationState<T>(T);

impl<T> ComponentState for CompositingLayerAnimationState<T>
where
    T: CompositingLayerAnimation,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn advance(&mut self, elapsed_ms: f32) -> bool {
        self.0.advance(elapsed_ms)
    }

    fn wants_frame(&self) -> bool {
        self.0.wants_frame()
    }

    fn frame_interval_ms(&self) -> u64 {
        self.0.frame_interval_ms()
    }
}

type CompositingLayerProjection = fn(&dyn ComponentState) -> CompositingLayerSpec;

#[derive(Clone)]
pub(crate) enum ComponentStateBinding {
    Component {
        owner: ComponentId,
        invalidation_id: UiId,
    },
    CompositingLayer {
        target_id: UiId,
        project: CompositingLayerProjection,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedNodeUpdate {
    CompositingLayer(CompositingLayerSpec),
}

pub(crate) struct ComponentStateInvalidation {
    pub state_id: UiId,
    pub target_id: UiId,
    pub owner: Option<ComponentId>,
    pub frame_interval_ms: u64,
    pub retained_update: Option<RetainedNodeUpdate>,
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
            Some(ComponentStateBinding::Component {
                owner,
                invalidation_id,
            }),
            f,
        )
    }

    pub(crate) fn with_mut_for_compositing_layer<T, R>(
        &self,
        id: &UiId,
        target_id: UiId,
        f: impl FnOnce(&mut T) -> R,
    ) -> (R, CompositingLayerSpec, bool)
    where
        T: CompositingLayerAnimation,
    {
        fn project<T>(state: &dyn ComponentState) -> CompositingLayerSpec
        where
            T: CompositingLayerAnimation,
        {
            state
                .as_any()
                .downcast_ref::<CompositingLayerAnimationState<T>>()
                .expect("compositing layer animation type mismatch for UiId")
                .0
                .compositing_layer_spec()
        }

        self.with_mut_inner(
            id,
            Some(ComponentStateBinding::CompositingLayer {
                target_id,
                project: project::<T>,
            }),
            |state: &mut CompositingLayerAnimationState<T>| {
                let result = f(&mut state.0);
                (
                    result,
                    state.0.compositing_layer_spec(),
                    state.0.wants_frame(),
                )
            },
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

    pub(crate) fn requested_frame_interval_ms(&self) -> Option<u64> {
        self.states
            .borrow()
            .values()
            .filter(|entry| entry.state.wants_frame())
            .map(|entry| entry.state.frame_interval_ms().max(1))
            .min()
    }

    pub(crate) fn advance_invalidations(&self, elapsed_ms: f32) -> Vec<ComponentStateInvalidation> {
        let mut dirty = Vec::new();
        for (id, entry) in self.states.borrow_mut().iter_mut() {
            if entry.state.advance(elapsed_ms) {
                let (target_id, owner, retained_update) = match entry.binding.as_ref() {
                    Some(ComponentStateBinding::Component {
                        owner,
                        invalidation_id,
                    }) => (invalidation_id.clone(), Some(*owner), None),
                    Some(ComponentStateBinding::CompositingLayer { target_id, project }) => (
                        target_id.clone(),
                        None,
                        Some(RetainedNodeUpdate::CompositingLayer(project(
                            entry.state.as_ref(),
                        ))),
                    ),
                    None => (id.clone(), None, None),
                };
                dirty.push(ComponentStateInvalidation {
                    state_id: id.clone(),
                    target_id,
                    owner,
                    frame_interval_ms: entry.state.frame_interval_ms().max(1),
                    retained_update,
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
#[path = "component_state_test.rs"]
mod tests;
