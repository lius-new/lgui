use std::{
    any::{type_name, Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use super::{
    ComponentId, ComponentTree, EffectRegistry, HookId, HookSlotKind, IntoEffectCleanup, UiEffect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ProviderKey {
    component: ComponentId,
    value_type: TypeId,
}

thread_local! {
    static CURRENT_CONTEXT: RefCell<Vec<CurrentContext>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct CurrentContext {
    registry: ContextRegistry,
    consumer: ComponentId,
}

struct StagedListenerEffect {
    component: ComponentId,
    index: usize,
    deps: Box<dyn Any>,
    deps_equal: fn(&dyn Any, &dyn Any) -> bool,
    run: Box<dyn FnOnce() -> Option<UiEffect> + 'static>,
}

#[derive(Clone, Default)]
pub struct ContextRegistry {
    inner: Rc<ContextRegistryState>,
}

#[derive(Default)]
struct ContextRegistryState {
    providers: RefCell<HashMap<ProviderKey, Box<dyn Any>>>,
    active: RefCell<HashMap<TypeId, Vec<ProviderKey>>>,
    consumers: RefCell<HashMap<ProviderKey, HashSet<ComponentId>>>,
    provider_rollback: RefCell<HashMap<ProviderKey, Option<Box<dyn Any>>>>,
    consumer_rollback: RefCell<Option<HashMap<ProviderKey, HashSet<ComponentId>>>>,
    listener_counts: RefCell<HashMap<ComponentId, usize>>,
    listener_count_rollback: RefCell<Option<HashMap<ComponentId, usize>>>,
    listener_rendered: RefCell<HashSet<ComponentId>>,
    listener_next: RefCell<HashMap<ComponentId, usize>>,
    staged_listener_effects: RefCell<Vec<StagedListenerEffect>>,
    render_active: Cell<bool>,
}

pub struct ContextProviderGuard<'a> {
    registry: &'a ContextRegistry,
    value_type: TypeId,
}

pub(crate) struct CurrentContextGuard;

/// Reads a typed value from the nearest provider and subscribes the component
/// currently being rendered to provider changes.
pub fn use_context<T>() -> T
where
    T: Clone + 'static,
{
    try_use_context::<T>().unwrap_or_else(|| {
        panic!(
            "missing active context value `{}`; context hooks may only run while rendering a component",
            type_name::<T>()
        )
    })
}

/// Tries to read a typed value from the nearest provider for the component
/// currently being rendered.
pub fn try_use_context<T>() -> Option<T>
where
    T: Clone + 'static,
{
    let current = CURRENT_CONTEXT.with(|stack| stack.borrow().last().cloned())?;
    current.registry.read(current.consumer)
}

impl ContextRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_render(&self) {
        self.restore_render_state();
        self.inner.active.borrow_mut().clear();
        *self.inner.consumer_rollback.borrow_mut() = Some(self.inner.consumers.borrow().clone());
        *self.inner.listener_count_rollback.borrow_mut() =
            Some(self.inner.listener_counts.borrow().clone());
        self.inner.listener_rendered.borrow_mut().clear();
        self.inner.listener_next.borrow_mut().clear();
        self.inner.staged_listener_effects.borrow_mut().clear();
        self.inner.render_active.set(true);
    }

    pub fn begin_component(&self, component: ComponentId) {
        self.inner.consumers.borrow_mut().retain(|_, consumers| {
            consumers.remove(&component);
            !consumers.is_empty()
        });
        self.inner.listener_rendered.borrow_mut().insert(component);
        self.inner.listener_next.borrow_mut().insert(component, 0);
    }

    pub(crate) fn validate_listener_hooks(&self) {
        let rendered = self.inner.listener_rendered.borrow();
        let next = self.inner.listener_next.borrow();
        let counts = self.inner.listener_counts.borrow();
        for component in rendered.iter().copied() {
            let current = next.get(&component).copied().unwrap_or_default();
            if let Some(previous) = counts.get(&component) {
                assert_eq!(
                    *previous, current,
                    "component {component} changed its receiver-free listener hook count from {previous} to {current}"
                );
            }
        }
        drop(counts);
        drop(next);
        drop(rendered);
    }

    pub(crate) fn commit_listener_effects(
        &self,
        components: &ComponentTree,
        effects: &EffectRegistry,
    ) {
        for staged in self.inner.staged_listener_effects.borrow_mut().drain(..) {
            if !components.is_alive(staged.component) {
                continue;
            }
            effects.register_erased(
                HookId::new(staged.component, staged.index, HookSlotKind::Listener),
                staged.deps,
                staged.deps_equal,
                staged.run,
            );
        }
    }

    pub fn end_render(&self, components: &ComponentTree) {
        debug_assert!(
            self.inner.active.borrow().values().all(Vec::is_empty),
            "context provider stack was not balanced"
        );
        self.inner.active.borrow_mut().clear();
        self.inner
            .providers
            .borrow_mut()
            .retain(|key, _| components.is_alive(key.component));
        self.inner.consumers.borrow_mut().retain(|key, consumers| {
            if !components.is_alive(key.component) {
                return false;
            }
            consumers.retain(|consumer| components.is_alive(*consumer));
            !consumers.is_empty()
        });
        self.inner.provider_rollback.borrow_mut().clear();
        self.inner.consumer_rollback.borrow_mut().take();
        {
            let rendered = self.inner.listener_rendered.borrow();
            let next = self.inner.listener_next.borrow();
            let mut counts = self.inner.listener_counts.borrow_mut();
            for component in rendered.iter().copied() {
                counts.insert(component, next.get(&component).copied().unwrap_or_default());
            }
            counts.retain(|component, _| components.is_alive(*component));
        }
        self.inner.listener_count_rollback.borrow_mut().take();
        self.inner.listener_rendered.borrow_mut().clear();
        self.inner.listener_next.borrow_mut().clear();
        self.inner.staged_listener_effects.borrow_mut().clear();
        self.inner.render_active.set(false);
    }

    pub fn abort_render(&self, _components: &ComponentTree) {
        self.inner.active.borrow_mut().clear();
        self.inner.staged_listener_effects.borrow_mut().clear();
        self.inner.listener_rendered.borrow_mut().clear();
        self.inner.listener_next.borrow_mut().clear();
        if let Some(counts) = self.inner.listener_count_rollback.borrow_mut().take() {
            *self.inner.listener_counts.borrow_mut() = counts;
        }
        self.restore_render_state();
        self.inner.render_active.set(false);
    }

    pub fn provide<T>(
        &self,
        owner: ComponentId,
        value: T,
        components: &ComponentTree,
    ) -> ContextProviderGuard<'_>
    where
        T: Clone + PartialEq + 'static,
    {
        let value_type = TypeId::of::<T>();
        let key = ProviderKey {
            component: owner,
            value_type,
        };
        let changed = {
            let mut providers = self.inner.providers.borrow_mut();
            if self.inner.render_active.get()
                && !self.inner.provider_rollback.borrow().contains_key(&key)
            {
                let previous = providers.get(&key).map(|current| {
                    Box::new(
                        current
                            .downcast_ref::<T>()
                            .unwrap_or_else(|| {
                                panic!("context provider type mismatch for `{}`", type_name::<T>())
                            })
                            .clone(),
                    ) as Box<dyn Any>
                });
                self.inner
                    .provider_rollback
                    .borrow_mut()
                    .insert(key, previous);
            }
            match providers.get_mut(&key) {
                Some(current) => {
                    let current = current.downcast_mut::<T>().unwrap_or_else(|| {
                        panic!("context provider type mismatch for `{}`", type_name::<T>())
                    });
                    if current == &value {
                        false
                    } else {
                        *current = value;
                        true
                    }
                }
                None => {
                    providers.insert(key, Box::new(value));
                    true
                }
            }
        };
        if changed {
            if let Some(consumers) = self.inner.consumers.borrow().get(&key) {
                for consumer in consumers {
                    components.mark_dirty(*consumer);
                }
            }
        }
        self.inner
            .active
            .borrow_mut()
            .entry(value_type)
            .or_default()
            .push(key);
        ContextProviderGuard {
            registry: self,
            value_type,
        }
    }

    pub fn read<T>(&self, consumer: ComponentId) -> Option<T>
    where
        T: Clone + 'static,
    {
        let value_type = TypeId::of::<T>();
        let key = self
            .inner
            .active
            .borrow()
            .get(&value_type)
            .and_then(|providers| providers.last())
            .copied()?;
        self.inner
            .consumers
            .borrow_mut()
            .entry(key)
            .or_default()
            .insert(consumer);
        Some(
            self.inner
                .providers
                .borrow()
                .get(&key)
                .and_then(|value| value.downcast_ref::<T>())
                .unwrap_or_else(|| panic!("context value type mismatch for `{}`", type_name::<T>()))
                .clone(),
        )
    }

    pub fn clear(&self) {
        self.inner.providers.borrow_mut().clear();
        self.inner.active.borrow_mut().clear();
        self.inner.consumers.borrow_mut().clear();
        self.inner.provider_rollback.borrow_mut().clear();
        self.inner.consumer_rollback.borrow_mut().take();
        self.inner.listener_counts.borrow_mut().clear();
        self.inner.listener_count_rollback.borrow_mut().take();
        self.inner.listener_rendered.borrow_mut().clear();
        self.inner.listener_next.borrow_mut().clear();
        self.inner.staged_listener_effects.borrow_mut().clear();
        self.inner.render_active.set(false);
    }

    fn stage_current_listener<D, F, R>(&self, component: ComponentId, deps: D, effect: F)
    where
        D: Clone + PartialEq + 'static,
        F: FnOnce() -> R + 'static,
        R: IntoEffectCleanup,
    {
        assert!(
            self.inner.render_active.get(),
            "receiver-free `listen` may only be called while rendering a component"
        );
        let index = {
            let mut next = self.inner.listener_next.borrow_mut();
            let index = next.get(&component).copied().unwrap_or_default();
            next.insert(component, index + 1);
            index
        };
        self.inner
            .staged_listener_effects
            .borrow_mut()
            .push(StagedListenerEffect {
                component,
                index,
                deps: Box::new(deps),
                deps_equal: listener_deps_equal::<D>,
                run: Box::new(move || effect().into_cleanup()),
            });
    }

    pub(crate) fn enter_current(&self, consumer: ComponentId) -> CurrentContextGuard {
        CURRENT_CONTEXT.with(|stack| {
            stack.borrow_mut().push(CurrentContext {
                registry: self.clone(),
                consumer,
            });
        });
        CurrentContextGuard
    }

    fn restore_render_state(&self) {
        let rollback = std::mem::take(&mut *self.inner.provider_rollback.borrow_mut());
        let mut providers = self.inner.providers.borrow_mut();
        for (key, previous) in rollback {
            match previous {
                Some(previous) => {
                    providers.insert(key, previous);
                }
                None => {
                    providers.remove(&key);
                }
            }
        }
        drop(providers);
        if let Some(consumers) = self.inner.consumer_rollback.borrow_mut().take() {
            *self.inner.consumers.borrow_mut() = consumers;
        }
    }
}

impl Drop for ContextProviderGuard<'_> {
    fn drop(&mut self) {
        let mut active = self.registry.inner.active.borrow_mut();
        let stack = active
            .get_mut(&self.value_type)
            .expect("context provider stack disappeared");
        stack.pop().expect("context provider stack underflow");
    }
}

impl Drop for CurrentContextGuard {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with(|stack| {
            stack
                .borrow_mut()
                .pop()
                .expect("current context stack underflow");
        });
    }
}

pub(crate) fn stage_current_listener<D, F, R>(deps: D, effect: F)
where
    D: Clone + PartialEq + 'static,
    F: FnOnce() -> R + 'static,
    R: IntoEffectCleanup,
{
    let current = CURRENT_CONTEXT
        .with(|stack| stack.borrow().last().cloned())
        .unwrap_or_else(|| {
            panic!("receiver-free `listen` may only be called while rendering a component")
        });
    current
        .registry
        .stage_current_listener(current.consumer, deps, effect);
}

fn listener_deps_equal<D>(left: &dyn Any, right: &dyn Any) -> bool
where
    D: PartialEq + 'static,
{
    let left = left
        .downcast_ref::<D>()
        .unwrap_or_else(|| panic!("listener dependency type changed between renders"));
    let right = right
        .downcast_ref::<D>()
        .unwrap_or_else(|| panic!("listener dependency type changed during render"));
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::UiId;

    #[test]
    fn nested_provider_restores_outer_value() {
        let components = ComponentTree::new();
        components.begin_render();
        let outer = components.root(UiId::owned("outer"), "outer");
        let inner = components.positioned_child(outer, 1, 0, "inner");
        let consumer = components.positioned_child(inner, 2, 0, "consumer");
        let contexts = ContextRegistry::new();
        contexts.begin_render();

        let outer_guard = contexts.provide(outer, 1_u32, &components);
        assert_eq!(contexts.read::<u32>(consumer), Some(1));
        {
            let _inner_guard = contexts.provide(inner, 2_u32, &components);
            assert_eq!(contexts.read::<u32>(consumer), Some(2));
        }
        assert_eq!(contexts.read::<u32>(consumer), Some(1));
        drop(outer_guard);
    }

    #[test]
    fn changed_provider_invalidates_only_recorded_consumers() {
        let components = ComponentTree::new();
        let contexts = ContextRegistry::new();

        components.begin_render();
        contexts.begin_render();
        let provider = components.root(UiId::owned("provider"), "provider");
        let consumer = components.positioned_child(provider, 1, 0, "consumer");
        let unrelated = components.positioned_child(provider, 1, 1, "unrelated");
        {
            let _guard = contexts.provide(provider, 1_u32, &components);
            assert_eq!(contexts.read::<u32>(consumer), Some(1));
        }
        components.finish_component(consumer);
        components.finish_component(unrelated);
        components.finish_component(provider);
        components.end_render();
        contexts.end_render(&components);
        assert!(!components.is_dirty(consumer));
        assert!(!components.is_dirty(unrelated));

        components.begin_render();
        contexts.begin_render();
        let provider = components.root(UiId::owned("provider"), "provider");
        let _guard = contexts.provide(provider, 2_u32, &components);

        assert!(components.is_dirty(consumer));
        assert!(!components.is_dirty(unrelated));
    }

    #[test]
    fn abort_render_restores_provider_values_and_consumer_subscriptions() {
        let components = ComponentTree::new();
        let contexts = ContextRegistry::new();

        components.begin_render();
        contexts.begin_render();
        let provider = components.root(UiId::owned("provider"), "provider");
        let consumer = components.positioned_child(provider, 1, 0, "consumer");
        {
            let _guard = contexts.provide(provider, 1_u32, &components);
            assert_eq!(contexts.read::<u32>(consumer), Some(1));
        }
        components.finish_component(consumer);
        components.finish_component(provider);
        components.end_render();
        contexts.end_render(&components);

        components.begin_render();
        contexts.begin_render();
        let provider = components.root(UiId::owned("provider"), "provider");
        {
            let _guard = contexts.provide(provider, 2_u32, &components);
        }
        contexts.abort_render(&components);
        components.abort_render();

        components.begin_render();
        contexts.begin_render();
        let provider = components.root(UiId::owned("provider"), "provider");
        let consumer = components.positioned_child(provider, 1, 0, "consumer");
        {
            let _guard = contexts.provide(provider, 1_u32, &components);
            assert_eq!(contexts.read::<u32>(consumer), Some(1));
        }
        assert!(!components.is_dirty(consumer));
    }
}
