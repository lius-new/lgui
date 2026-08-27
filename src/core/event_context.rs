use std::{future::Future, sync::Arc};

use crate::application::{ApplicationContext, WindowHandle, WindowId, WindowManager};

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

    #[cfg(feature = "store")]
    pub fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.application.read_store(read)
    }

    #[cfg(feature = "store")]
    pub fn update_store<T, R>(
        &mut self,
        reason: impl Into<std::borrow::Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: crate::store::StoreUnit,
    {
        let result = self.application.update_store(reason, update);
        self.mark_changed(false);
        result
    }

    #[cfg(feature = "router")]
    pub fn navigate<R>(&mut self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        if self.application.router::<R>().navigate(route).is_some() {
            self.mark_changed(true);
        }
    }

    #[cfg(feature = "router")]
    pub fn replace<R>(&mut self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        if self.application.router::<R>().replace(route).is_some() {
            self.mark_changed(true);
        }
    }

    #[cfg(feature = "router")]
    pub fn back<R>(&mut self)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        if self.application.router::<R>().back().is_some() {
            self.mark_changed(true);
        }
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
        let context = UiAsyncContext {
            application: self.application.clone(),
        };
        let _ = self.spawn(handler(context));
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
    pub fn application(&self) -> ApplicationContext {
        self.application.clone()
    }

    pub fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.application.resource::<T>()
    }

    #[cfg(feature = "store")]
    pub fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.application.read_store(read)
    }

    #[cfg(feature = "store")]
    pub fn update_store<T, R>(
        &self,
        reason: impl Into<std::borrow::Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.application.update_store(reason, update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reactive_changes_do_not_implicitly_request_a_global_frame() {
        let mut context = UiEventContext::new(
            ApplicationContext::empty(),
            WindowId::new("reactive-change"),
        );

        context.mark_changed(true);

        assert!(context.flags().changed);
        assert!(context.flags().route_changed);
        assert!(!context.flags().needs_frame);

        context.request_frame();
        assert!(context.flags().needs_frame);
    }
}
