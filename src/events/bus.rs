use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{Event, EventSubscription};

type EventListener = Arc<dyn Fn(&dyn Any) + Send + Sync + 'static>;

pub(super) struct EventTopic {
    pub(super) listeners: HashMap<u64, EventListener>,
}

#[derive(Default)]
pub(super) struct EventBusInner {
    pub(super) topics: Mutex<HashMap<TypeId, EventTopic>>,
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
        EventSubscription::new(&self.inner, TypeId::of::<E>(), id)
    }
}
