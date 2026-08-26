use std::sync::{Arc, Mutex};

use crate::core::UiRect;

use super::{
    definition::StoreDefinition,
    notification::{StoreNotification, StoreObserver, Subscription, SubscriptionToken},
    registry::StoreRegistry,
    unit::{StoreInvalidation, StoreLifecycleEvent, StoreUnit},
    StoreMutation,
};

pub struct StoreRuntime {
    registry: Mutex<StoreRegistry>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreFrameResult {
    pub changed: bool,
    pub should_continue: bool,
}

impl StoreRuntime {
    pub fn new(registry: StoreRegistry) -> Self {
        Self {
            registry: Mutex::new(registry),
        }
    }

    pub fn read<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit,
    {
        let registry = self.registry.lock().expect("store poisoned");
        let domain = registry
            .get::<T>()
            .expect("missing domain store in store runtime");
        read(domain)
    }

    pub fn read_defined<T, R>(
        &self,
        definition: StoreDefinition<T>,
        read: impl FnOnce(&T) -> R,
    ) -> R
    where
        T: Send + Sync + 'static,
    {
        let registry = self.registry.lock().expect("store poisoned");
        let store = registry
            .get_defined(definition)
            .unwrap_or_else(|| panic!("missing store definition `{}`", definition.key()));
        read(store)
    }

    pub fn update<T>(
        &self,
        reason: &'static str,
        mutate: impl FnOnce(&mut T) -> StoreMutation,
    ) -> bool
    where
        T: StoreUnit,
    {
        let notification = self
            .registry
            .lock()
            .expect("store poisoned")
            .update(reason, mutate);
        let changed = notification.is_some();
        self.dispatch_notifications(notification);
        changed
    }

    pub fn update_defined<T>(
        &self,
        definition: StoreDefinition<T>,
        reason: &'static str,
        mutate: impl FnOnce(&mut T),
    ) -> bool
    where
        T: Send + Sync + 'static,
    {
        let notification = self
            .registry
            .lock()
            .expect("store poisoned")
            .update_defined(definition, reason, mutate);
        let changed = notification.is_some();
        self.dispatch_notifications(notification);
        changed
    }

    pub fn handle_lifecycle_event(&self, event: StoreLifecycleEvent) -> bool {
        let notifications = self
            .registry
            .lock()
            .expect("store poisoned")
            .handle_lifecycle_event(event);
        let changed = !notifications.is_empty();
        self.dispatch_notifications(notifications);
        changed
    }

    pub fn advance_animations(&self, elapsed_ms: f32) -> bool {
        let notifications = self
            .registry
            .lock()
            .expect("store poisoned")
            .advance_animations(elapsed_ms);
        let changed = !notifications.is_empty();
        self.dispatch_notifications(notifications);
        changed
    }

    pub fn has_running_animations(&self) -> bool {
        self.registry
            .lock()
            .expect("store poisoned")
            .has_running_animations()
    }

    pub fn advance_frame(&self, elapsed_ms: f32) -> StoreFrameResult {
        let changed = self.advance_animations(elapsed_ms);
        StoreFrameResult {
            changed,
            should_continue: self.has_running_animations(),
        }
    }

    pub fn presenter_bridge(&self) -> StorePresenterBridge<'_> {
        StorePresenterBridge { runtime: self }
    }

    pub fn subscribe(
        &self,
        subscription: Subscription,
        observer: Arc<dyn StoreObserver>,
    ) -> SubscriptionToken {
        self.registry
            .lock()
            .expect("store poisoned")
            .subscribe(subscription, observer)
    }

    pub fn unsubscribe(&self, token: SubscriptionToken) -> bool {
        self.registry
            .lock()
            .expect("store poisoned")
            .unsubscribe(token)
    }

    fn dispatch_notifications(&self, notifications: impl IntoIterator<Item = StoreNotification>) {
        for notification in notifications {
            let observers = self
                .registry
                .lock()
                .expect("store poisoned")
                .observers_for(&notification);
            for observer in observers {
                observer.on_store_notification(&notification);
            }
        }
    }
}

pub struct StorePresenterBridge<'a> {
    runtime: &'a StoreRuntime,
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    };

    use super::*;
    use crate::store::{create, StoreNotification, StoreObserver, Subscription};

    #[derive(Default)]
    struct TestStore {
        value: u32,
    }

    impl StoreUnit for TestStore {
        const KEY: &'static str = "test.runtime";
    }

    struct ReadingObserver {
        runtime: Weak<StoreRuntime>,
        observed: Mutex<Vec<u32>>,
    }

    impl StoreObserver for ReadingObserver {
        fn on_store_notification(&self, _notification: &StoreNotification) {
            let runtime = self.runtime.upgrade().expect("test runtime");
            let value = runtime.read::<TestStore, _>(|store| store.value);
            self.observed.lock().expect("test observer").push(value);
        }
    }

    #[derive(Default)]
    struct CounterStore {
        count: i32,
    }

    fn create_counter() -> CounterStore {
        CounterStore { count: 1 }
    }

    fn increment(counter: &mut CounterStore) {
        counter.count += 1;
    }

    fn set_count(counter: &mut CounterStore, count: i32) {
        counter.count = count;
    }

    const COUNTER_STORE: StoreDefinition<CounterStore> = create("test.counter", create_counter);
    const INCREMENT: crate::store::StoreAction<CounterStore> = COUNTER_STORE.action(increment);
    const SET_COUNT: crate::store::StoreActionWith<CounterStore, i32> =
        COUNTER_STORE.action_with(set_count);

    #[derive(Default)]
    struct CountingObserver {
        notifications: AtomicUsize,
    }

    impl StoreObserver for CountingObserver {
        fn on_store_notification(&self, _notification: &StoreNotification) {
            self.notifications.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn observers_can_read_the_updated_store_without_relocking_the_registry() {
        let mut registry = StoreRegistry::new();
        registry.create(TestStore::default());
        let runtime = Arc::new(StoreRuntime::new(registry));
        let observer = Arc::new(ReadingObserver {
            runtime: Arc::downgrade(&runtime),
            observed: Mutex::new(Vec::new()),
        });
        runtime.subscribe(Subscription::store(TestStore::KEY), observer.clone());

        assert!(runtime.update::<TestStore>("test.update", |store| {
            store.value = 7;
            StoreMutation::changed()
        }));

        assert_eq!(*observer.observed.lock().expect("test observer"), vec![7]);
    }

    #[test]
    fn created_store_actions_update_state_and_notify_store_subscribers() {
        let mut registry = StoreRegistry::new();
        registry.register(COUNTER_STORE);
        let runtime = StoreRuntime::new(registry);
        let observer = Arc::new(CountingObserver::default());
        runtime.subscribe(Subscription::store(COUNTER_STORE.key()), observer.clone());

        assert!(INCREMENT.call_in(&runtime));
        assert!(SET_COUNT.call_in(&runtime, 7));

        assert_eq!(
            runtime.read_defined(COUNTER_STORE, |counter| counter.count),
            7
        );
        assert_eq!(observer.notifications.load(Ordering::Relaxed), 2);
    }
}

impl StorePresenterBridge<'_> {
    pub fn subscribe(&self, observer: Arc<dyn StoreObserver>) -> Vec<SubscriptionToken> {
        let mut registry = self.runtime.registry.lock().expect("store poisoned");
        registry
            .presenter_subscriptions()
            .into_iter()
            .map(|subscription| registry.subscribe(subscription, observer.clone()))
            .collect()
    }

    pub fn unsubscribe(&self, tokens: Vec<SubscriptionToken>) {
        let mut registry = self.runtime.registry.lock().expect("store poisoned");
        for token in tokens {
            let _ = registry.unsubscribe(token);
        }
    }

    pub fn collect_invalidations(
        &self,
        notification: &StoreNotification,
        viewport: UiRect,
    ) -> Vec<StoreInvalidation> {
        self.runtime
            .registry
            .lock()
            .expect("store poisoned")
            .collect_presenter_invalidations(notification, viewport)
    }
}
