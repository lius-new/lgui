use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use super::{
    definition::StoreDefinition,
    notification::{StoreNotification, StoreObserverRef, Subscription, SubscriptionToken},
    unit::{
        StoreInvalidation, StoreInvalidationContext, StoreInvalidationSet, StoreLifecycleEvent,
        StoreMutation, StoreUnit,
    },
};

pub trait AnyStoreUnit: Send + Sync {
    fn key(&self) -> &'static str;
    fn handle_lifecycle_event(&mut self, event: StoreLifecycleEvent) -> StoreMutation;
    fn advance_animations(&mut self, elapsed_ms: f32) -> StoreMutation;
    fn has_running_animations(&self) -> bool;
    fn presenter_subscriptions(&self) -> Vec<Subscription>;
    fn collect_presenter_invalidations(
        &self,
        context: StoreInvalidationContext<'_>,
        invalidations: &mut StoreInvalidationSet,
    );
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct StoreEntry<T: StoreUnit> {
    unit: T,
}

struct DefinedStoreEntry<T> {
    key: &'static str,
    unit: T,
}

impl<T: StoreUnit> AnyStoreUnit for StoreEntry<T> {
    fn key(&self) -> &'static str {
        T::KEY
    }

    fn handle_lifecycle_event(&mut self, event: StoreLifecycleEvent) -> StoreMutation {
        self.unit.handle_lifecycle_event(event)
    }

    fn advance_animations(&mut self, elapsed_ms: f32) -> StoreMutation {
        self.unit.advance_animations(elapsed_ms)
    }

    fn has_running_animations(&self) -> bool {
        self.unit.has_running_animations()
    }

    fn presenter_subscriptions(&self) -> Vec<Subscription> {
        T::presenter_subscriptions()
    }

    fn collect_presenter_invalidations(
        &self,
        context: StoreInvalidationContext<'_>,
        invalidations: &mut StoreInvalidationSet,
    ) {
        T::collect_presenter_invalidations(context, invalidations);
    }

    fn as_any(&self) -> &dyn Any {
        &self.unit
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.unit
    }
}

impl<T> AnyStoreUnit for DefinedStoreEntry<T>
where
    T: Send + Sync + 'static,
{
    fn key(&self) -> &'static str {
        self.key
    }

    fn handle_lifecycle_event(&mut self, _event: StoreLifecycleEvent) -> StoreMutation {
        StoreMutation::new()
    }

    fn advance_animations(&mut self, _elapsed_ms: f32) -> StoreMutation {
        StoreMutation::new()
    }

    fn has_running_animations(&self) -> bool {
        false
    }

    fn presenter_subscriptions(&self) -> Vec<Subscription> {
        Vec::new()
    }

    fn collect_presenter_invalidations(
        &self,
        _context: StoreInvalidationContext<'_>,
        _invalidations: &mut StoreInvalidationSet,
    ) {
    }

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

    pub fn create<T: StoreUnit>(&mut self, unit: T) {
        self.insert::<T>(T::KEY, Box::new(StoreEntry { unit }));
    }

    pub fn register<T>(&mut self, definition: StoreDefinition<T>)
    where
        T: Send + Sync + 'static,
    {
        let key = definition.key();
        assert!(!key.is_empty(), "store key must not be empty");
        self.insert::<T>(
            key,
            Box::new(DefinedStoreEntry {
                key,
                unit: definition.initialize(),
            }),
        );
    }

    fn insert<T>(&mut self, key: &'static str, entry: Box<dyn AnyStoreUnit>)
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        assert!(
            !self.stores.contains_key(&type_id),
            "duplicate store unit type `{}`",
            key
        );
        assert!(
            !self.keys.contains_key(key),
            "duplicate store key `{}`",
            key
        );
        self.keys.insert(key, type_id);
        self.stores.insert(type_id, entry);
    }

    pub fn get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.stores
            .get(&TypeId::of::<T>())
            .and_then(|store| store.as_any().downcast_ref::<T>())
    }

    pub fn get_defined<T>(&self, definition: StoreDefinition<T>) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        let registered_type = self.keys.get(definition.key())?;
        if *registered_type != TypeId::of::<T>() {
            return None;
        }
        self.get::<T>()
    }

    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: Send + Sync + 'static,
    {
        self.stores
            .get_mut(&TypeId::of::<T>())
            .and_then(|store| store.as_any_mut().downcast_mut::<T>())
    }

    pub fn update<T: StoreUnit>(
        &mut self,
        reason: &'static str,
        mutate: impl FnOnce(&mut T) -> StoreMutation,
    ) -> Option<StoreNotification> {
        let Some(unit) = self.get_mut::<T>() else {
            return None;
        };
        let mutation = mutate(unit);
        if mutation.is_empty() {
            return None;
        }
        Some(mutation.into_notification(T::KEY, reason))
    }

    pub fn update_defined<T>(
        &mut self,
        definition: StoreDefinition<T>,
        reason: &'static str,
        mutate: impl FnOnce(&mut T),
    ) -> Option<StoreNotification>
    where
        T: Send + Sync + 'static,
    {
        if self.keys.get(definition.key()) != Some(&TypeId::of::<T>()) {
            return None;
        }
        let unit = self.get_mut::<T>()?;
        mutate(unit);
        Some(StoreMutation::changed().into_notification(definition.key(), reason))
    }

    pub fn handle_lifecycle_event(&mut self, event: StoreLifecycleEvent) -> Vec<StoreNotification> {
        let mut notifications = Vec::new();
        for store in self.stores.values_mut() {
            let mutation = store.handle_lifecycle_event(event);
            if mutation.is_empty() {
                continue;
            }
            notifications.push(mutation.into_notification(store.key(), "lifecycle"));
        }

        notifications
    }

    pub fn advance_animations(&mut self, elapsed_ms: f32) -> Vec<StoreNotification> {
        let mut notifications = Vec::new();
        for store in self.stores.values_mut() {
            let mutation = store.advance_animations(elapsed_ms);
            if mutation.is_empty() {
                continue;
            }
            notifications.push(mutation.into_notification(store.key(), "animation.advance"));
        }

        notifications
    }

    pub fn has_running_animations(&self) -> bool {
        self.stores
            .values()
            .any(|store| store.has_running_animations())
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

    pub fn presenter_subscriptions(&self) -> Vec<Subscription> {
        self.stores
            .values()
            .flat_map(|store| store.presenter_subscriptions())
            .collect()
    }

    pub fn collect_presenter_invalidations(
        &self,
        notification: &StoreNotification,
        viewport: crate::core::UiRect,
    ) -> Vec<StoreInvalidation> {
        let mut invalidations = StoreInvalidationSet::new();
        let Some(type_id) = self.keys.get(notification.store_key) else {
            invalidations.all();
            return invalidations.into_requests();
        };
        let Some(store) = self.stores.get(type_id) else {
            invalidations.all();
            return invalidations.into_requests();
        };
        store.collect_presenter_invalidations(
            StoreInvalidationContext {
                viewport,
                notification,
            },
            &mut invalidations,
        );
        if invalidations.is_empty() {
            invalidations.all();
        }
        invalidations.into_requests()
    }

    pub fn observers_for(&self, notification: &StoreNotification) -> Vec<StoreObserverRef> {
        self.subscriptions
            .iter()
            .filter(|entry| self.matches(&entry.subscription, notification))
            .map(|entry| entry.observer.clone())
            .collect()
    }

    fn matches(&self, subscription: &Subscription, notification: &StoreNotification) -> bool {
        subscription.store_key == notification.store_key
    }
}
