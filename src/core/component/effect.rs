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
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::core::{ComponentTree, HookSlotKind, UiId};

    fn effect_id(tree: &ComponentTree) -> HookId {
        let component = tree.root(UiId::owned("effect-owner"), "effect-owner");
        HookId::new(component, 0, HookSlotKind::Effect)
    }

    #[test]
    fn dependency_change_cleans_up_before_next_effect() {
        let registry = EffectRegistry::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let tree = ComponentTree::new();
        tree.begin_render();
        let id = effect_id(&tree);

        registry.begin_frame();
        let first_events = Rc::clone(&events);
        registry.register(id, 1_u32, move || {
            first_events.borrow_mut().push("run-1");
            let cleanup_events = Rc::clone(&first_events);
            move || cleanup_events.borrow_mut().push("cleanup-1")
        });
        tree.finish_component(id.component());
        tree.end_render();
        registry.end_frame(&tree);
        registry.run_pending();

        tree.begin_render();
        let id = effect_id(&tree);
        registry.begin_frame();
        let second_events = Rc::clone(&events);
        registry.register(id, 2_u32, move || {
            second_events.borrow_mut().push("run-2");
        });
        tree.finish_component(id.component());
        tree.end_render();
        registry.end_frame(&tree);
        registry.run_pending();

        assert_eq!(&*events.borrow(), &["run-1", "cleanup-1", "run-2"]);
    }

    #[test]
    fn unmount_drops_an_effect_that_never_committed() {
        let registry = EffectRegistry::new();
        let runs = Rc::new(Cell::new(0));
        let tree = ComponentTree::new();
        tree.begin_render();
        let id = effect_id(&tree);

        registry.begin_frame();
        let effect_runs = Rc::clone(&runs);
        registry.register(id, (), move || {
            effect_runs.set(effect_runs.get() + 1);
        });
        tree.finish_component(id.component());
        tree.end_render();
        registry.end_frame(&tree);

        tree.begin_render();
        tree.end_render();
        registry.begin_frame();
        registry.end_frame(&tree);
        registry.run_pending();

        assert_eq!(runs.get(), 0);
    }

    #[test]
    fn committed_effect_cleans_up_when_its_component_unmounts() {
        let registry = EffectRegistry::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let tree = ComponentTree::new();

        tree.begin_render();
        registry.begin_frame();
        let id = effect_id(&tree);
        let effect_events = Rc::clone(&events);
        registry.register(id, (), move || {
            effect_events.borrow_mut().push("mount");
            let cleanup_events = Rc::clone(&effect_events);
            move || cleanup_events.borrow_mut().push("unmount")
        });
        tree.finish_component(id.component());
        tree.end_render();
        registry.end_frame(&tree);
        registry.run_pending();

        tree.begin_render();
        tree.end_render();
        registry.begin_frame();
        registry.end_frame(&tree);
        registry.run_pending();

        assert_eq!(&*events.borrow(), &["mount", "unmount"]);
    }

    #[test]
    fn abandoned_render_does_not_replace_a_committed_effect() {
        let registry = EffectRegistry::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let tree = ComponentTree::new();

        tree.begin_render();
        registry.begin_frame();
        let id = effect_id(&tree);
        let first_events = Rc::clone(&events);
        registry.register(id, 1_u32, move || {
            first_events.borrow_mut().push("run-1");
            let cleanup_events = Rc::clone(&first_events);
            move || cleanup_events.borrow_mut().push("cleanup-1")
        });
        tree.finish_component(id.component());
        tree.end_render();
        registry.end_frame(&tree);
        registry.run_pending();

        tree.begin_render();
        registry.begin_frame();
        let id = effect_id(&tree);
        let abandoned_events = Rc::clone(&events);
        registry.register(id, 2_u32, move || {
            abandoned_events.borrow_mut().push("run-abandoned");
        });
        tree.abandon_component(id.component());

        registry.begin_frame();
        registry.run_pending();
        assert_eq!(&*events.borrow(), &["run-1"]);

        registry.clear();
        assert_eq!(&*events.borrow(), &["run-1", "cleanup-1"]);
    }
}
