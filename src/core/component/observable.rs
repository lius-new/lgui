use std::sync::Arc;

use super::UiEffect;

pub type ObservableListener = Arc<dyn Fn() + Send + Sync + 'static>;

pub struct Observable<T> {
    id: u64,
    read: Arc<dyn Fn() -> T + Send + Sync + 'static>,
    subscribe: Arc<dyn Fn(ObservableListener) -> UiEffect + Send + Sync + 'static>,
}

impl<T> Clone for Observable<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            read: Arc::clone(&self.read),
            subscribe: Arc::clone(&self.subscribe),
        }
    }
}

impl<T> Observable<T> {
    pub fn new(
        id: u64,
        read: impl Fn() -> T + Send + Sync + 'static,
        subscribe: impl Fn(ObservableListener) -> UiEffect + Send + Sync + 'static,
    ) -> Self {
        Self {
            id,
            read: Arc::new(read),
            subscribe: Arc::new(subscribe),
        }
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn read(&self) -> T {
        (self.read)()
    }

    pub fn subscribe(&self, listener: ObservableListener) -> UiEffect {
        (self.subscribe)(listener)
    }
}
