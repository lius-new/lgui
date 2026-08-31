use std::{future::Future, pin::Pin};

use crate::core::UiAsyncContext;

/// A typed, Application-scoped event.
///
/// `NAME` is diagnostic metadata; event delivery uses the Rust event type.
pub trait Event: Clone + Send + Sync + 'static {
    const NAME: &'static str;
}

pub type EventFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub trait AsyncEventHandler<E>: Send + Sync + 'static
where
    E: Event,
{
    fn call(&self, context: UiAsyncContext, event: E) -> EventFuture;
}

impl<E, F, Fut> AsyncEventHandler<E> for F
where
    E: Event,
    F: Fn(UiAsyncContext, E) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn call(&self, context: UiAsyncContext, event: E) -> EventFuture {
        Box::pin((self)(context, event))
    }
}
