use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::resources::Resources;

use super::super::{
    definition::StoreDefinition,
    subscription::{StoreNotification, StoreObserverRef, Subscription, SubscriptionToken},
    StoreUnit,
};

trait AnyStoreUnit: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct StoreEntry<T> {
    unit: T,
}

impl<T> AnyStoreUnit for StoreEntry<T>
where
    T: Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn Any {
        &self.unit
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.unit
    }
}

struct SubscriptionEntry {
    token: SubscriptionToken,
    subscription: Subscription,
    observer: StoreObserverRef,
}

pub struct StoreRegistry {
    stores: HashMap<TypeId, Box<dyn AnyStoreUnit>>,
    keys: HashMap<&'static str, TypeId>,
    subscriptions: Vec<SubscriptionEntry>,
    next_token: u64,
}

impl StoreRegistry {
    pub fn new() -> Self {
        Self {
            stores: HashMap::new(),
            keys: HashMap::new(),
            subscriptions: Vec::new(),
            next_token: 1,
        }
    }

    pub fn ensure<T>(&mut self, resources: &Resources)
    where
        T: StoreUnit,
    {
        if self.stores.contains_key(&TypeId::of::<T>()) {
            return;
        }
        self.insert::<T>(T::KEY, T::create(resources));
    }

    pub fn ensure_defined<T>(&mut self, definition: StoreDefinition<T>)
    where
        T: Send + Sync + 'static,
    {
        if self.stores.contains_key(&TypeId::of::<T>()) {
            return;
        }
        self.insert::<T>(definition.key(), definition.initialize());
    }

    fn insert<T>(&mut self, key: &'static str, unit: T)
    where
        T: Send + Sync + 'static,
    {
        assert!(!key.is_empty(), "store key must not be empty");
        let type_id = TypeId::of::<T>();
        assert!(!self.keys.contains_key(key), "duplicate store key `{key}`");
        self.keys.insert(key, type_id);
        self.stores.insert(type_id, Box::new(StoreEntry { unit }));
    }

    pub fn get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.stores
            .get(&TypeId::of::<T>())
            .and_then(|store| store.as_any().downcast_ref::<T>())
    }

    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: Send + Sync + 'static,
    {
        self.stores
            .get_mut(&TypeId::of::<T>())
            .and_then(|store| store.as_any_mut().downcast_mut::<T>())
    }

    pub fn subscribe(
        &mut self,
        subscription: Subscription,
        observer: StoreObserverRef,
    ) -> SubscriptionToken {
        let token = SubscriptionToken(self.next_token);
        self.next_token += 1;
        self.subscriptions.push(SubscriptionEntry {
            token: token.clone(),
            subscription,
            observer,
        });
        token
    }

    pub fn unsubscribe(&mut self, token: SubscriptionToken) -> bool {
        let len_before = self.subscriptions.len();
        self.subscriptions.retain(|entry| entry.token != token);
        self.subscriptions.len() != len_before
    }

    pub fn observers_for(&self, notification: &StoreNotification) -> Vec<StoreObserverRef> {
        self.subscriptions
            .iter()
            .filter(|entry| entry.subscription.store_key == notification.store_key)
            .map(|entry| entry.observer.clone())
            .collect()
    }
}

impl Default for StoreRegistry {
    fn default() -> Self {
        Self::new()
    }
}
