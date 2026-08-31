use std::{any::Any, cell::RefCell, collections::HashMap};

use super::{ComponentTree, HookId};

pub type UiEffect = Box<dyn FnOnce() + 'static>;

pub trait IntoEffectCleanup {
    fn into_cleanup(self) -> Option<UiEffect>;
}

impl IntoEffectCleanup for () {
    fn into_cleanup(self) -> Option<UiEffect> {
        None
    }
}

impl<F> IntoEffectCleanup for F
where
    F: FnOnce() + 'static,
{
    fn into_cleanup(self) -> Option<UiEffect> {
        Some(Box::new(self))
    }
}

struct EffectEntry {
    deps: Box<dyn Any>,
    generation: u64,
    cleanup: Option<UiEffect>,
}

struct StagedEffect {
    id: HookId,
    deps: Box<dyn Any>,
    deps_equal: fn(&dyn Any, &dyn Any) -> bool,
    run: Box<dyn FnOnce() -> Option<UiEffect> + 'static>,
}

struct PendingEffect {
    id: HookId,
    generation: u64,
    run: Box<dyn FnOnce() -> Option<UiEffect> + 'static>,
}

#[derive(Default)]
pub struct EffectRegistry {
    entries: RefCell<HashMap<HookId, EffectEntry>>,
    staged: RefCell<Vec<StagedEffect>>,
    pending_cleanups: RefCell<Vec<UiEffect>>,
    pending_effects: RefCell<Vec<PendingEffect>>,
}

impl EffectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<D, F, R>(&self, id: HookId, deps: D, effect: F)
    where
        D: Clone + PartialEq + 'static,
        F: FnOnce() -> R + 'static,
        R: IntoEffectCleanup,
    {
        let mut staged = self.staged.borrow_mut();
        if staged.iter().any(|candidate| candidate.id == id) {
            panic!("effect hook `{id}` was registered more than once in one render");
        }
        staged.push(StagedEffect {
            id,
            deps: Box::new(deps),
            deps_equal: deps_equal::<D>,
            run: Box::new(move || effect().into_cleanup()),
        });
    }

    pub fn begin_frame(&self) {
        self.staged.borrow_mut().clear();
    }

    pub fn abort_frame(&self) {
        self.staged.borrow_mut().clear();
    }

    pub fn end_frame(&self, components: &ComponentTree) {
        let staged = self.staged.borrow_mut().drain(..).collect::<Vec<_>>();
        for candidate in staged {
            if !components.is_alive(candidate.id.component()) {
                continue;
            }
            let generation = {
                let mut entries = self.entries.borrow_mut();
                match entries.get_mut(&candidate.id) {
                    Some(entry) => {
                        if (candidate.deps_equal)(entry.deps.as_ref(), candidate.deps.as_ref()) {
                            continue;
                        }
                        if let Some(cleanup) = entry.cleanup.take() {
                            self.pending_cleanups.borrow_mut().push(cleanup);
                        }
                        entry.deps = candidate.deps;
                        entry.generation = entry.generation.wrapping_add(1);
                        entry.generation
                    }
                    None => {
                        entries.insert(
                            candidate.id,
                            EffectEntry {
                                deps: candidate.deps,
                                generation: 1,
                                cleanup: None,
                            },
                        );
                        1
                    }
                }
            };
            self.pending_effects.borrow_mut().push(PendingEffect {
                id: candidate.id,
                generation,
                run: candidate.run,
            });
        }

        let removed = {
            let mut entries = self.entries.borrow_mut();
            let mut removed_ids: Vec<HookId> = entries
                .keys()
                .filter(|id| !components.is_alive(id.component()))
                .cloned()
                .collect();
            removed_ids.sort_unstable();
            removed_ids
                .into_iter()
                .filter_map(|id| entries.remove(&id).and_then(|entry| entry.cleanup))
                .collect::<Vec<_>>()
        };
        self.pending_cleanups.borrow_mut().extend(removed);
        self.pending_effects
            .borrow_mut()
            .retain(|pending| components.is_alive(pending.id.component()));
    }

    pub fn run_pending(&self) {
        let cleanups = self
            .pending_cleanups
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        for cleanup in cleanups {
            cleanup();
        }

        let effects = self
            .pending_effects
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        for pending in effects {
            let is_current = self
                .entries
                .borrow()
                .get(&pending.id)
                .is_some_and(|entry| entry.generation == pending.generation);
            if !is_current {
                continue;
            }
            let cleanup = (pending.run)();
            if let Some(entry) = self.entries.borrow_mut().get_mut(&pending.id) {
                if entry.generation == pending.generation {
                    entry.cleanup = cleanup;
                }
            }
        }
    }

    pub fn clear(&self) {
        self.staged.borrow_mut().clear();
        self.pending_effects.borrow_mut().clear();
        let mut cleanups = self
            .pending_cleanups
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        cleanups.extend(
            self.entries
                .borrow_mut()
                .drain()
                .filter_map(|(_, entry)| entry.cleanup),
        );
        for cleanup in cleanups {
            cleanup();
        }
    }
}

fn deps_equal<D>(left: &dyn Any, right: &dyn Any) -> bool
where
    D: PartialEq + 'static,
{
    let left = left
        .downcast_ref::<D>()
        .unwrap_or_else(|| panic!("effect dependency type changed between renders"));
    let right = right
        .downcast_ref::<D>()
        .unwrap_or_else(|| panic!("effect dependency type changed during render"));
    left == right
}

#[cfg(test)]
#[path = "effect/tests.rs"]
mod tests;
