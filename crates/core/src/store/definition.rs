use std::marker::PhantomData;

pub struct StoreDefinition<T> {
    key: &'static str,
    initialize: fn() -> T,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for StoreDefinition<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StoreDefinition<T> {}

impl<T> StoreDefinition<T> {
    pub const fn key(self) -> &'static str {
        self.key
    }

    pub(crate) fn initialize(self) -> T {
        (self.initialize)()
    }
}

pub const fn create<T>(key: &'static str, initialize: fn() -> T) -> StoreDefinition<T> {
    StoreDefinition {
        key,
        initialize,
        marker: PhantomData,
    }
}
