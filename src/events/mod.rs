//! Typed, application-scoped event delivery.

mod bus;
mod contract;
mod subscription;

use std::{future::Future, sync::Arc};

pub(crate) use bus::EventBus;
pub use contract::{AsyncEventHandler, Event, EventFuture, EventKey, InvalidEventKey};
pub use subscription::EventSubscription;

/// Emits a typed payload to listeners registered for `key` in the active
/// Application.
///
/// The returned Future completes after synchronous listeners return and
/// asynchronous listeners have been scheduled. It does not wait for
/// asynchronous listener work to finish.
pub async fn emit<T>(key: EventKey<T>, payload: T) -> usize
where
    T: Clone + Send + Sync + 'static,
{
    crate::application::current_application("emit").emit_keyed(&key, payload)
}

/// Declares a typed listener owned by the component currently being rendered.
///
/// The subscription is installed after a successful present and is removed
/// automatically when the component unmounts. Like other hooks, `listen` must
/// be called unconditionally and in a stable order.
pub fn listen<T>(key: EventKey<T>, listener: impl Fn(T) + Send + Sync + 'static)
where
    T: Clone + Send + Sync + 'static,
{
    listen_with(key, (), listener);
}

/// Declares a component-owned listener that is replaced when `deps` changes.
pub fn listen_with<T, D>(key: EventKey<T>, deps: D, listener: impl Fn(T) + Send + Sync + 'static)
where
    T: Clone + Send + Sync + 'static,
    D: Clone + PartialEq + 'static,
{
    let application = crate::core::use_context::<crate::application::ApplicationContext>();
    let effect_deps = (key.clone(), deps);
    crate::core::stage_current_listener(effect_deps, move || {
        let subscription = application.subscribe_keyed(key, listener);
        move || drop(subscription)
    });
}

/// Declares a component-owned listener whose callback Future runs on the
/// Application executor.
pub fn listen_async<T, F, Fut>(key: EventKey<T>, listener: F)
where
    T: Clone + Send + Sync + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    listen_async_with(key, (), listener);
}

/// Declares an asynchronous component-owned listener that is replaced when
/// `deps` changes.
pub fn listen_async_with<T, D, F, Fut>(key: EventKey<T>, deps: D, listener: F)
where
    T: Clone + Send + Sync + 'static,
    D: Clone + PartialEq + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let application = crate::core::use_context::<crate::application::ApplicationContext>();
    let effect_deps = (key.clone(), deps);
    let listener = Arc::new(listener);
    crate::core::stage_current_listener(effect_deps, move || {
        let task_application = application.clone();
        let subscription = application.subscribe_keyed(key, move |payload| {
            let _ = task_application.spawn(listener(payload));
        });
        move || drop(subscription)
    });
}

#[cfg(test)]
mod tests;
