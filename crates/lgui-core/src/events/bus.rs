use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{EventKey, EventSubscription};

type EventListener = Arc<dyn Fn(&dyn Any) + Send + Sync + 'static>;

pub(super) struct EventTopic {
    pub(super) payload_type: TypeId,
    pub(super) payload_name: &'static str,
    pub(super) listeners: HashMap<u64, EventListener>,
}

#[derive(Default)]
pub(super) struct EventBusInner {
    pub(super) topics: Mutex<HashMap<Arc<str>, EventTopic>>,
    next_subscription: AtomicU64,
}

#[derive(Clone, Default)]
pub(crate) struct EventBus {
    inner: Arc<EventBusInner>,
}

impl EventBus {
    pub(crate) fn emit<T>(&self, key: &EventKey<T>, payload: T) -> usize
    where
        T: Clone + Send + Sync + 'static,
    {
        let (listeners, mismatched_payload) = {
            let topics = self.inner.topics.lock().expect("event bus poisoned");
            match topics.get(key.as_str()) {
                Some(topic) if topic.payload_type != TypeId::of::<T>() => {
                    (Vec::new(), Some(topic.payload_name))
                }
                Some(topic) => (topic.listeners.values().cloned().collect::<Vec<_>>(), None),
                None => (Vec::new(), None),
            }
        };
        if let Some(expected) = mismatched_payload {
            panic_payload_type::<T>(key.as_str(), expected);
        }
        for listener in &listeners {
            listener(&payload);
        }
        listeners.len()
    }

    pub(crate) fn subscribe<T>(
        &self,
        key: EventKey<T>,
        listener: impl Fn(T) + Send + Sync + 'static,
    ) -> EventSubscription
    where
        T: Clone + Send + Sync + 'static,
    {
        let id = self
            .inner
            .next_subscription
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let listener = Arc::new(move |event: &dyn Any| {
            let event = event
                .downcast_ref::<T>()
                .unwrap_or_else(|| panic!("event payload type mismatch"));
            listener(event.clone());
        }) as EventListener;
        let name = key.shared_name();
        let mut topics = self.inner.topics.lock().expect("event bus poisoned");
        let mismatched_payload = topics.get(key.as_str()).and_then(|topic| {
            (topic.payload_type != TypeId::of::<T>()).then_some(topic.payload_name)
        });
        if let Some(expected) = mismatched_payload {
            drop(topics);
            panic_payload_type::<T>(key.as_str(), expected);
        }
        let topic = topics
            .entry(Arc::clone(&name))
            .or_insert_with(|| EventTopic {
                payload_type: TypeId::of::<T>(),
                payload_name: std::any::type_name::<T>(),
                listeners: HashMap::new(),
            });
        topic.listeners.insert(id, listener);
        drop(topics);
        EventSubscription::new(&self.inner, name, id)
    }

    #[cfg(test)]
    pub(crate) fn topic_count(&self) -> usize {
        self.inner.topics.lock().expect("event bus poisoned").len()
    }
}

fn panic_payload_type<T>(key: &str, expected: &'static str) -> !
where
    T: 'static,
{
    panic!(
        "event key `{key}` is already bound to payload type `{}` and cannot be used with `{}`",
        expected,
        std::any::type_name::<T>()
    );
}
