use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// Application-owned typed values shared by every component and window.
#[derive(Clone, Default)]
pub struct Resources {
    values: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
}

impl Resources {
    pub fn new() -> Self {
        Self::default()
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

    pub fn require<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.get::<T>()
            .unwrap_or_else(|| panic!("missing application resource `{}`", type_name::<T>()))
    }
}

#[cfg(test)]
#[path = "resources_test.rs"]
mod tests;
