use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::core::{Observable, ObservableListener, RenderCx, UiEffect};

use super::{
    StoreDefinition, StoreNotification, StoreObserver, StoreRuntime, StoreUnit, Subscription,
};

#[derive(Clone)]
pub struct StoreContext {
    runtime: Arc<StoreRuntime>,
}

impl StoreContext {
    pub fn new(runtime: Arc<StoreRuntime>) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &Arc<StoreRuntime> {
        &self.runtime
    }
}

impl PartialEq for StoreContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.runtime, &other.runtime)
    }
}

pub struct StoreAction<T> {
    store: StoreDefinition<T>,
    mutate: fn(&mut T),
}

impl<T> Clone for StoreAction<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StoreAction<T> {}

pub struct StoreActionWith<T, A> {
    store: StoreDefinition<T>,
    mutate: fn(&mut T, A),
}

impl<T, A> Clone for StoreActionWith<T, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, A> Copy for StoreActionWith<T, A> {}

pub struct BoundStoreAction<T> {
    runtime: Arc<StoreRuntime>,
    action: StoreAction<T>,
}

pub struct BoundStoreActionWith<T, A> {
    runtime: Arc<StoreRuntime>,
    action: StoreActionWith<T, A>,
}

impl<T> Clone for BoundStoreAction<T> {
    fn clone(&self) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime),
            action: self.action,
        }
    }
}

impl<T, A> Clone for BoundStoreActionWith<T, A> {
    fn clone(&self) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime),
            action: self.action,
        }
    }
}

impl<T> StoreDefinition<T>
where
    T: Send + Sync + 'static,
{
    pub fn select<S, F>(self, cx: &mut RenderCx<'_, '_>, selector: F) -> S
    where
        S: Clone + PartialEq + Send + 'static,
        F: Fn(&T) -> S + Send + Sync + 'static,
    {
        let context = cx.use_context::<StoreContext>();
        use_definition(cx, context.runtime, self, selector)
    }

    pub const fn action(self, mutate: fn(&mut T)) -> StoreAction<T> {
        StoreAction {
            store: self,
            mutate,
        }
    }

    pub const fn action_with<A>(self, mutate: fn(&mut T, A)) -> StoreActionWith<T, A> {
        StoreActionWith {
            store: self,
            mutate,
        }
    }
}

impl<T> StoreAction<T>
where
    T: Send + Sync + 'static,
{
    pub fn bind(self, cx: &mut RenderCx<'_, '_>) -> BoundStoreAction<T> {
        let context = cx.use_context::<StoreContext>();
        BoundStoreAction {
            runtime: context.runtime,
            action: self,
        }
    }

    pub fn call_in(self, runtime: &StoreRuntime) {
        runtime.update_defined(self.store, "store.action", self.mutate)
    }
}

impl<T, A> StoreActionWith<T, A>
where
    T: Send + Sync + 'static,
{
    pub fn bind(self, cx: &mut RenderCx<'_, '_>) -> BoundStoreActionWith<T, A> {
        let context = cx.use_context::<StoreContext>();
        BoundStoreActionWith {
            runtime: context.runtime,
            action: self,
        }
    }

    pub fn call_in(self, runtime: &StoreRuntime, argument: A) {
        runtime.update_defined(self.store, "store.action", move |store| {
            (self.mutate)(store, argument)
        })
    }
}

impl<T> BoundStoreAction<T>
where
    T: Send + Sync + 'static,
{
    pub fn call(&self) {
        self.action.call_in(&self.runtime)
    }
}

impl<T, A> BoundStoreActionWith<T, A>
where
    T: Send + Sync + 'static,
{
    pub fn call(&self, argument: A) {
        self.action.call_in(&self.runtime, argument)
    }
}

pub trait StoreHooks {
    fn use_store<T, S, F>(&mut self, selector: F) -> S
    where
        T: StoreUnit,
        S: Clone + PartialEq + Send + 'static,
        F: Fn(&T) -> S + Send + Sync + 'static;
}

impl StoreHooks for RenderCx<'_, '_> {
    fn use_store<T, S, F>(&mut self, selector: F) -> S
    where
        T: StoreUnit,
        S: Clone + PartialEq + Send + 'static,
        F: Fn(&T) -> S + Send + Sync + 'static,
    {
        let context = self.use_context::<StoreContext>();
        use_unit(self, context.runtime, selector)
    }
}

struct CallbackObserver {
    listener: ObservableListener,
}

impl StoreObserver for CallbackObserver {
    fn on_store_notification(&self, _notification: &StoreNotification) {
        (self.listener)();
    }
}

fn use_unit<T, S, F>(cx: &mut RenderCx<'_, '_>, runtime: Arc<StoreRuntime>, selector: F) -> S
where
    T: StoreUnit,
    S: Clone + PartialEq + Send + 'static,
    F: Fn(&T) -> S + Send + Sync + 'static,
{
    let read_runtime = Arc::clone(&runtime);
    let subscribe_runtime = runtime;
    let selector = Arc::new(selector);
    let source = Observable::new(
        observable_id::<T, F>(),
        move || read_runtime.read::<T, _>(|store| selector(store)),
        move |listener| subscribe::<T>(Arc::clone(&subscribe_runtime), listener),
    );
    cx.use_observable(source, clone_selected::<S>)
}

fn use_definition<T, S, F>(
    cx: &mut RenderCx<'_, '_>,
    runtime: Arc<StoreRuntime>,
    store: StoreDefinition<T>,
    selector: F,
) -> S
where
    T: Send + Sync + 'static,
    S: Clone + PartialEq + Send + 'static,
    F: Fn(&T) -> S + Send + Sync + 'static,
{
    let read_runtime = Arc::clone(&runtime);
    let subscribe_runtime = runtime;
    let selector = Arc::new(selector);
    let source = Observable::new(
        observable_id::<T, F>(),
        move || read_runtime.read_defined(store, |value| selector(value)),
        move |listener| subscribe_definition(Arc::clone(&subscribe_runtime), store, listener),
    );
    cx.use_observable(source, clone_selected::<S>)
}

fn subscribe<T>(runtime: Arc<StoreRuntime>, listener: ObservableListener) -> UiEffect
where
    T: StoreUnit,
{
    let observer = Arc::new(CallbackObserver { listener });
    let token = runtime.subscribe(Subscription::store(T::KEY), observer);
    Box::new(move || {
        runtime.unsubscribe(token);
    })
}

fn subscribe_definition<T>(
    runtime: Arc<StoreRuntime>,
    store: StoreDefinition<T>,
    listener: ObservableListener,
) -> UiEffect
where
    T: Send + Sync + 'static,
{
    let observer = Arc::new(CallbackObserver { listener });
    let token = runtime.subscribe(Subscription::store(store.key()), observer);
    Box::new(move || {
        runtime.unsubscribe(token);
    })
}

fn observable_id<T: 'static, F: 'static>() -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::any::TypeId::of::<T>().hash(&mut hasher);
    std::any::TypeId::of::<F>().hash(&mut hasher);
    hasher.finish()
}

fn clone_selected<T: Clone>(value: &T) -> T {
    value.clone()
}

#[cfg(test)]
#[path = "hooks_test.rs"]
mod tests;
