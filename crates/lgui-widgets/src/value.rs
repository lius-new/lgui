//! Controlled widget values backed by component State or an application store.
use lgui_core::core::State;
use std::sync::Arc;

#[derive(Clone)]
pub struct WidgetValue<T> {
    read: Arc<dyn Fn() -> T + Send + Sync>,
    write: Arc<dyn Fn(T) + Send + Sync>,
}
impl<T> WidgetValue<T> {
    pub fn new(
        read: impl Fn() -> T + Send + Sync + 'static,
        write: impl Fn(T) + Send + Sync + 'static,
    ) -> Self {
        Self {
            read: Arc::new(read),
            write: Arc::new(write),
        }
    }
    pub fn get(&self) -> T {
        (self.read)()
    }
    pub fn set(&self, value: T) {
        (self.write)(value)
    }
    pub fn update(&self, edit: impl FnOnce(&mut T)) {
        let mut value = self.get();
        edit(&mut value);
        self.set(value);
    }
}
impl<T: Clone + Send + 'static> From<State<T>> for WidgetValue<T> {
    fn from(state: State<T>) -> Self {
        let reader = state.clone();
        Self::new(move || reader.get(), move |value| state.set(value))
    }
}
