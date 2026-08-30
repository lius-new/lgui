use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
};

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

type EventListener = Arc<dyn Fn(&dyn Any) + Send + Sync + 'static>;

struct EventTopic {
    listeners: HashMap<u64, EventListener>,
}

#[derive(Default)]
struct EventBusInner {
    topics: Mutex<HashMap<TypeId, EventTopic>>,
    next_subscription: AtomicU64,
}

#[derive(Clone, Default)]
pub(crate) struct EventBus {
    inner: Arc<EventBusInner>,
}

impl EventBus {
    pub(crate) fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        let listeners = self
            .inner
            .topics
            .lock()
            .expect("event bus poisoned")
            .get(&TypeId::of::<E>())
            .map(|topic| topic.listeners.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for listener in &listeners {
            listener(&event);
        }
        listeners.len()
    }

    pub(crate) fn subscribe<E>(
        &self,
        listener: impl Fn(E) + Send + Sync + 'static,
    ) -> EventSubscription
    where
        E: Event,
    {
        let id = self
            .inner
            .next_subscription
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let listener = Arc::new(move |event: &dyn Any| {
            let event = event
                .downcast_ref::<E>()
                .unwrap_or_else(|| panic!("event `{}` payload type mismatch", E::NAME));
            listener(event.clone());
        }) as EventListener;
        self.inner
            .topics
            .lock()
            .expect("event bus poisoned")
            .entry(TypeId::of::<E>())
            .or_insert_with(|| EventTopic {
                listeners: HashMap::new(),
            })
            .listeners
            .insert(id, listener);
        EventSubscription {
            bus: Arc::downgrade(&self.inner),
            event_type: TypeId::of::<E>(),
            id,
        }
    }
}

pub struct EventSubscription {
    bus: Weak<EventBusInner>,
    event_type: TypeId,
    id: u64,
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        let Some(bus) = self.bus.upgrade() else {
            return;
        };
        let mut topics = bus.topics.lock().expect("event bus poisoned");
        let remove_topic = topics.get_mut(&self.event_type).is_some_and(|topic| {
            topic.listeners.remove(&self.id);
            topic.listeners.is_empty()
        });
        if remove_topic {
            topics.remove(&self.event_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Clone)]
    struct CounterEvent(usize);

    impl Event for CounterEvent {
        const NAME: &'static str = "test.counter";
    }

    #[test]
    fn events_are_typed_and_subscriptions_stop_on_drop() {
        let bus = EventBus::default();
        let total = Arc::new(AtomicUsize::new(0));
        let subscription = bus.subscribe::<CounterEvent>({
            let total = Arc::clone(&total);
            move |event| {
                total.fetch_add(event.0, Ordering::SeqCst);
            }
        });

        assert_eq!(bus.emit(CounterEvent(3)), 1);
        assert_eq!(total.load(Ordering::SeqCst), 3);
        drop(subscription);
        assert_eq!(bus.emit(CounterEvent(4)), 0);
        assert_eq!(total.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn event_buses_are_isolated() {
        let first = EventBus::default();
        let second = EventBus::default();
        let deliveries = Arc::new(AtomicUsize::new(0));
        let _subscription = first.subscribe::<CounterEvent>({
            let deliveries = Arc::clone(&deliveries);
            move |_| {
                deliveries.fetch_add(1, Ordering::SeqCst);
            }
        });

        assert_eq!(second.emit(CounterEvent(1)), 0);
        assert_eq!(deliveries.load(Ordering::SeqCst), 0);
    }
}
