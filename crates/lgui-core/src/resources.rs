use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    sync::{Arc, RwLock, Weak},
};

/// Application-owned typed values shared by every component and window.
#[derive(Clone, Default)]
pub struct Resources {
    values: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
}

#[derive(Clone)]
pub struct WeakResources {
    values: Weak<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
}

impl Resources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn downgrade(&self) -> WeakResources {
        WeakResources {
            values: Arc::downgrade(&self.values),
        }
    }

    pub fn provide<T>(&self, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.values
            .write()
            .expect("application resources poisoned")
            .insert(TypeId::of::<T>(), Arc::new(value));
    }

    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.values
            .read()
            .expect("application resources poisoned")
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|value| value.downcast::<T>().ok())
    }

    pub fn get_or_insert_with<T>(&self, create: impl FnOnce() -> T) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        let mut values = self.values.write().expect("application resources poisoned");
        values
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(create()))
            .clone()
            .downcast::<T>()
            .unwrap_or_else(|_| {
                panic!(
                    "application resource type mismatch for `{}`",
                    type_name::<T>()
                )
            })
    }

    pub fn require<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.get::<T>()
            .unwrap_or_else(|| panic!("missing application resource `{}`", type_name::<T>()))
    }
}

impl WeakResources {
    pub fn upgrade(&self) -> Option<Resources> {
        self.values.upgrade().map(|values| Resources { values })
    }
}

#[cfg(test)]
#[path = "resources_test.rs"]
mod tests;
