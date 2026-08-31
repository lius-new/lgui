use std::sync::Arc;

use crate::{
    core::{InputEvent, RuntimeOutput, UiTask, UiTaskSpawner, UiWake},
    session::UiSession,
};

#[derive(Clone)]
pub struct WakeHandle {
    wake: UiWake,
}

impl WakeHandle {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            wake: Arc::new(wake),
        }
    }

    pub fn wake(&self) {
        (self.wake)();
    }

    pub fn as_ui_wake(&self) -> UiWake {
        Arc::clone(&self.wake)
    }
}

pub trait InputSink {
    fn handle_input(&mut self, input: InputEvent) -> RuntimeOutput;
}

impl InputSink for UiSession {
    fn handle_input(&mut self, input: InputEvent) -> RuntimeOutput {
        UiSession::handle_input(self, input)
    }
}

pub fn task_spawner(executor: impl Fn(UiTask) + Send + Sync + 'static) -> UiTaskSpawner {
    Arc::new(executor)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn wake_handle_is_cloneable_and_backend_neutral() {
        let count = Arc::new(AtomicUsize::new(0));
        let wake = WakeHandle::new({
            let count = Arc::clone(&count);
            move || {
                count.fetch_add(1, Ordering::Relaxed);
            }
        });

        wake.clone().wake();
        (wake.as_ui_wake())();

        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
