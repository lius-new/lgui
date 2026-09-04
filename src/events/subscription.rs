use std::sync::{Arc, Weak};

use super::bus::EventBusInner;

pub struct EventSubscription {
    bus: Weak<EventBusInner>,
    key: Arc<str>,
    id: u64,
}

impl EventSubscription {
    pub(super) fn new(bus: &Arc<EventBusInner>, key: Arc<str>, id: u64) -> Self {
        Self {
            bus: Arc::downgrade(bus),
            key,
            id,
        }
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        let Some(bus) = self.bus.upgrade() else {
            return;
        };
        let mut topics = bus.topics.lock().expect("event bus poisoned");
        let remove_topic = topics.get_mut(self.key.as_ref()).is_some_and(|topic| {
            topic.listeners.remove(&self.id);
            topic.listeners.is_empty()
        });
        if remove_topic {
            topics.remove(self.key.as_ref());
        }
    }
}
