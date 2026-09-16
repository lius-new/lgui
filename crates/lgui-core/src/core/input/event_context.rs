use std::{future::Future, sync::Arc};

use crate::{
    application::ApplicationContext,
    window::{WindowHandle, WindowId, WindowManager},
};
use crate::{
    command::{Command, CommandHandle},
    events::Event,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiEventFlags {
    pub consumed: bool,
    pub changed: bool,
    pub route_changed: bool,
    pub needs_frame: bool,
    pub default_prevented: bool,
}

/// Generic application, Store, Router, task and window access passed to every event handler.
pub struct UiEventContext {
    application: ApplicationContext,
    window: WindowHandle,
    flags: UiEventFlags,
    propagation_stopped: bool,
}

#[derive(Clone)]
pub struct UiAsyncContext {
    application: ApplicationContext,
    window: Option<WindowHandle>,
}

impl UiEventContext {
    pub fn new(application: ApplicationContext, window_id: WindowId) -> Self {
        let window = WindowHandle::new(window_id, application.windows());
        Self {
            application,
            window,
            flags: UiEventFlags::default(),
            propagation_stopped: false,
        }
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

    pub fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.application.try_resource::<T>()
    }

    pub fn command<C>(&self) -> CommandHandle<C>
    where
        C: Command,
    {
        self.application.command::<C>()
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.application.emit(event)
    }

    pub fn spawn(&mut self, task: impl Future<Output = ()> + Send + 'static) -> bool {
        self.mark_consumed();
        self.application.spawn(task)
    }

    pub fn spawn_or_else(
        &mut self,
        task: impl Future<Output = ()> + Send + 'static,
        unavailable: impl FnOnce(),
    ) {
        if !self.spawn(task) {
            unavailable();
        }
    }

    pub fn spawn_async<Fut>(&mut self, handler: impl FnOnce(UiAsyncContext) -> Fut + Send + 'static)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let context = self.async_context();
        let _ = self.spawn(handler(context));
    }

    pub fn async_context(&self) -> UiAsyncContext {
        UiAsyncContext {
            application: self.application.clone(),
            window: Some(self.window.clone()),
        }
    }

    pub fn window(&self) -> WindowHandle {
        self.window.clone()
    }

    pub fn windows(&self) -> WindowManager {
        self.application.windows()
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
        self.flags.consumed = true;
    }

    pub fn prevent_default(&mut self) {
        self.flags.default_prevented = true;
        self.flags.consumed = true;
    }

    pub fn propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    pub fn default_prevented(&self) -> bool {
        self.flags.default_prevented
    }

    pub fn flags(&self) -> UiEventFlags {
        self.flags
    }

    pub fn mark_consumed(&mut self) {
        self.flags.consumed = true;
    }

    pub fn mark_changed(&mut self, route_changed: bool) {
        self.flags.consumed = true;
        self.flags.changed = true;
        self.flags.route_changed |= route_changed;
    }

    pub fn request_frame(&mut self) {
        self.flags.needs_frame = true;
    }
}

impl UiAsyncContext {
    pub(crate) fn application_only(application: ApplicationContext) -> Self {
        Self {
            application,
            window: None,
        }
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

    pub fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.application.try_resource::<T>()
    }

    pub fn command<C>(&self) -> CommandHandle<C>
    where
        C: Command,
    {
        self.application.command::<C>()
    }

    pub async fn invoke<C>(&self, args: C::Args) -> Result<C::Output, C::Error>
    where
        C: Command,
    {
        self.application.invoke::<C>(args).await
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.application.emit(event)
    }

    pub fn window(&self) -> Option<WindowHandle> {
        self.window.clone()
    }

    pub fn windows(&self) -> WindowManager {
        self.application.windows()
    }
}

#[cfg(test)]
#[path = "event_context_test.rs"]
mod tests;
