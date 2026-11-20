//! Fine-grained updates: a [`Signal`] value and the node bindings that follow it.
//!
//! A [`State`](super::State) change re-runs the component that owns it and rebuilds
//! its element subtree. A `Signal` does not: components do not subscribe to it. Instead an
//! element binds node properties to the signal (`Element::bind`). When the signal changes, only
//! the bound nodes are patched in place and repainted. Nothing is rebuilt or compared.
//!
//! Read a signal for its *initial* value while rendering (`signal.get()`); keep it current
//! afterwards only through bindings. A render that re-runs reads the current value again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use super::{HostTree, UiId, UiUpdateQueue};
use crate::core::UiNode;

/// Rewrites a node in place.
pub type NodePatch = Arc<dyn Fn(&mut UiNode) + Send + Sync + 'static>;
type Apply = NodePatch;

/// A value whose changes patch bound nodes instead of re-rendering a component.
#[derive(Clone)]
pub struct Signal<T> {
    value: Arc<Mutex<T>>,
    updates: Arc<UiUpdateQueue>,
}

impl<T> Signal<T>
where
    T: Clone + Send + 'static,
{
    pub(super) fn new(value: Arc<Mutex<T>>, updates: Arc<UiUpdateQueue>) -> Self {
        Self { value, updates }
    }

    pub fn get(&self) -> T {
        self.value.lock().expect("signal value poisoned").clone()
    }

    /// Identifies the signal for the life of its component.
    pub(crate) fn key(&self) -> usize {
        Arc::as_ptr(&self.value) as *const () as usize
    }

    /// Stores `next`. Equal values change nothing and notify no one.
    pub fn set(&self, next: T)
    where
        T: PartialEq,
    {
        self.try_update(|value| {
            if *value == next {
                false
            } else {
                *value = next;
                true
            }
        });
    }

    /// Runs `update` and notifies the bound nodes when it returns `true`.
    pub fn try_update(&self, update: impl FnOnce(&mut T) -> bool) -> bool {
        let changed = update(&mut self.value.lock().expect("signal value poisoned"));
        if changed {
            self.updates.signal_changed(self.key());
        }
        changed
    }
}

/// One property patch of one element, driven by one signal.
#[derive(Clone)]
pub struct NodeBinding {
    pub(crate) key: usize,
    pub(crate) apply: Apply,
}

impl NodeBinding {
    pub(crate) fn new<T, F>(signal: &Signal<T>, patch: F) -> Self
    where
        T: Clone + Send + 'static,
        F: Fn(&T, &mut UiNode) + Send + Sync + 'static,
    {
        let signal = signal.clone();
        Self {
            key: signal.key(),
            apply: Arc::new(move |node| patch(&signal.get(), node)),
        }
    }
}

/// Which nodes follow which signal. Entries outlive a render so a reused component keeps its
/// bindings; they are replaced when the element is mounted again and dropped with their node.
#[derive(Default)]
pub struct SignalBindings {
    by_signal: RefCell<HashMap<usize, Vec<Registered>>>,
}

struct Registered {
    node: UiId,
    /// Position among the bindings of the same element and signal.
    slot: usize,
    apply: Apply,
}

impl SignalBindings {
    pub(crate) fn register(&self, node: UiId, slot: usize, binding: &NodeBinding) {
        let mut by_signal = self.by_signal.borrow_mut();
        let list = by_signal.entry(binding.key).or_default();
        match list
            .iter_mut()
            .find(|entry| entry.slot == slot && entry.node == node)
        {
            Some(entry) => entry.apply = Arc::clone(&binding.apply),
            None => list.push(Registered {
                node,
                slot,
                apply: Arc::clone(&binding.apply),
            }),
        }
    }

    /// The patches to apply for the signals that changed, for nodes still in `tree`.
    pub(crate) fn patches(
        &self,
        changed: HashSet<usize>,
        tree: &HostTree,
    ) -> Vec<(UiId, Apply)> {
        let mut by_signal = self.by_signal.borrow_mut();
        let mut patches = Vec::new();
        for key in changed {
            let Some(list) = by_signal.get_mut(&key) else {
                continue;
            };
            list.retain(|entry| tree.node(&entry.node).is_some());
            patches.extend(
                list.iter()
                    .map(|entry| (entry.node.clone(), Arc::clone(&entry.apply))),
            );
            if list.is_empty() {
                by_signal.remove(&key);
            }
        }
        patches
    }

    pub(crate) fn clear(&self) {
        self.by_signal.borrow_mut().clear();
    }
}
