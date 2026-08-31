//! Typed, application-scoped event delivery.

mod bus;
mod contract;
mod subscription;

pub(crate) use bus::EventBus;
pub use contract::{AsyncEventHandler, Event, EventFuture};
pub use subscription::EventSubscription;

#[cfg(test)]
mod tests;
