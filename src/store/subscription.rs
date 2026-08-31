use std::{borrow::Cow, sync::Arc};

#[derive(Clone, Debug)]
pub struct StoreNotification {
    pub store_key: &'static str,
    pub reason: Cow<'static, str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionToken(pub(crate) u64);

#[derive(Clone, Debug)]
pub struct Subscription {
    pub store_key: &'static str,
}

impl Subscription {
    pub fn store(store_key: &'static str) -> Self {
        Self { store_key }
    }
}

pub trait StoreObserver: Send + Sync {
    fn on_store_notification(&self, notification: &StoreNotification);
}

pub type StoreObserverRef = Arc<dyn StoreObserver>;
