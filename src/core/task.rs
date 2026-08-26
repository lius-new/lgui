use std::{future::Future, pin::Pin, sync::Arc};

pub type UiTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
pub type UiTaskSpawner = Arc<dyn Fn(UiTask) + Send + Sync + 'static>;

pub fn noop_task_spawner() -> UiTaskSpawner {
    Arc::new(|_| {})
}
