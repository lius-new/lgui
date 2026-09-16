use std::sync::Arc;

use crate::{application::ApplicationContext, events::Event};

#[derive(Clone)]
pub struct CommandContext {
    application: ApplicationContext,
}

impl CommandContext {
    pub(crate) fn new(application: ApplicationContext) -> Self {
        Self { application }
    }

    pub fn application(&self) -> ApplicationContext {
        self.application.clone()
    }

    pub fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.application.resource::<T>()
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.application.emit(event)
    }
}
