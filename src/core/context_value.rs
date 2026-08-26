use std::{
    any::{type_name, Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use super::{ComponentId, ComponentTree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ProviderKey {
    component: ComponentId,
    value_type: TypeId,
}

#[derive(Default)]
pub struct ContextRegistry {
    providers: RefCell<HashMap<ProviderKey, Box<dyn Any>>>,
    active: RefCell<HashMap<TypeId, Vec<ProviderKey>>>,
    consumers: RefCell<HashMap<ProviderKey, HashSet<ComponentId>>>,
    provider_rollback: RefCell<HashMap<ProviderKey, Option<Box<dyn Any>>>>,
    consumer_rollback: RefCell<Option<HashMap<ProviderKey, HashSet<ComponentId>>>>,
    render_active: Cell<bool>,
}

pub struct ContextProviderGuard<'a> {
    registry: &'a ContextRegistry,
    value_type: TypeId,
}

impl ContextRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_render(&self) {
        self.restore_render_state();
        self.active.borrow_mut().clear();
        *self.consumer_rollback.borrow_mut() = Some(self.consumers.borrow().clone());
        self.render_active.set(true);
    }

    pub fn begin_component(&self, component: ComponentId) {
        self.consumers.borrow_mut().retain(|_, consumers| {
            consumers.remove(&component);
            !consumers.is_empty()
        });
    }

    pub fn end_render(&self, components: &ComponentTree) {
        debug_assert!(
            self.active.borrow().values().all(Vec::is_empty),
            "context provider stack was not balanced"
        );
        self.active.borrow_mut().clear();
        self.providers
            .borrow_mut()
            .retain(|key, _| components.is_alive(key.component));
        self.consumers.borrow_mut().retain(|key, consumers| {
            if !components.is_alive(key.component) {
                return false;
            }
            consumers.retain(|consumer| components.is_alive(*consumer));
            !consumers.is_empty()
        });
        self.provider_rollback.borrow_mut().clear();
        self.consumer_rollback.borrow_mut().take();
        self.render_active.set(false);
    }

    pub fn abort_render(&self, _components: &ComponentTree) {
        self.active.borrow_mut().clear();
        self.restore_render_state();
        self.render_active.set(false);
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
            let mut providers = self.providers.borrow_mut();
            if self.render_active.get() && !self.provider_rollback.borrow().contains_key(&key) {
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
                self.provider_rollback.borrow_mut().insert(key, previous);
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
            if let Some(consumers) = self.consumers.borrow().get(&key) {
                for consumer in consumers {
                    components.mark_dirty(*consumer);
                }
            }
        }
        self.active
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
            .active
            .borrow()
            .get(&value_type)
            .and_then(|providers| providers.last())
            .copied()?;
        self.consumers
            .borrow_mut()
            .entry(key)
            .or_default()
            .insert(consumer);
        Some(
            self.providers
                .borrow()
                .get(&key)
                .and_then(|value| value.downcast_ref::<T>())
                .unwrap_or_else(|| panic!("context value type mismatch for `{}`", type_name::<T>()))
                .clone(),
        )
    }

    pub fn clear(&self) {
        self.providers.borrow_mut().clear();
        self.active.borrow_mut().clear();
        self.consumers.borrow_mut().clear();
        self.provider_rollback.borrow_mut().clear();
        self.consumer_rollback.borrow_mut().take();
        self.render_active.set(false);
    }

    fn restore_render_state(&self) {
        let rollback = std::mem::take(&mut *self.provider_rollback.borrow_mut());
        let mut providers = self.providers.borrow_mut();
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
        if let Some(consumers) = self.consumer_rollback.borrow_mut().take() {
            *self.consumers.borrow_mut() = consumers;
        }
    }
}

impl Drop for ContextProviderGuard<'_> {
    fn drop(&mut self) {
        let mut active = self.registry.active.borrow_mut();
        let stack = active
            .get_mut(&self.value_type)
            .expect("context provider stack disappeared");
        stack.pop().expect("context provider stack underflow");
    }
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
