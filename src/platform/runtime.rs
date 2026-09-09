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
#[path = "runtime_test.rs"]
mod tests;
