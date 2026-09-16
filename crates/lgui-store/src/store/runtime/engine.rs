use std::{
    borrow::Cow,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    core::UiWake,
    resources::{Resources, WeakResources},
};

use super::super::{
    definition::StoreDefinition,
    subscription::{StoreNotification, StoreObserver, Subscription, SubscriptionToken},
    StoreUnit,
};
use super::registry::StoreRegistry;

pub struct StoreRuntime {
    registry: Mutex<StoreRegistry>,
    resources: StoreResources,
    wake: RwLock<Option<UiWake>>,
}

enum StoreResources {
    Owned(Resources),
    Application(WeakResources),
}

impl StoreResources {
    fn get(&self) -> Resources {
        match self {
            Self::Owned(resources) => resources.clone(),
            Self::Application(resources) => resources
                .upgrade()
                .expect("application resources were dropped before the store runtime"),
        }
    }
}

impl StoreRuntime {
    pub fn new(resources: Resources) -> Self {
        Self {
            registry: Mutex::new(StoreRegistry::new()),
            resources: StoreResources::Owned(resources),
            wake: RwLock::new(None),
        }
    }

    pub fn for_application(resources: &Resources) -> Self {
        Self {
            registry: Mutex::new(StoreRegistry::new()),
            resources: StoreResources::Application(resources.downgrade()),
            wake: RwLock::new(None),
        }
    }

    pub fn set_wake(&self, wake: UiWake) {
        *self.wake.write().expect("store wake lock poisoned") = Some(wake);
    }

    pub fn clear_wake(&self) {
        *self.wake.write().expect("store wake lock poisoned") = None;
    }

    pub fn read<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit,
    {
        let mut registry = self.registry.lock().expect("store poisoned");
        registry.ensure::<T>(&self.resources.get());
        read(registry.get::<T>().expect("lazy store must exist"))
    }

    pub fn read_defined<T, R>(
        &self,
        definition: StoreDefinition<T>,
        read: impl FnOnce(&T) -> R,
    ) -> R
    where
        T: Send + Sync + 'static,
    {
        let mut registry = self.registry.lock().expect("store poisoned");
        registry.ensure_defined(definition);
        read(registry.get::<T>().expect("lazy defined store must exist"))
    }

    pub fn update<T, R>(
        &self,
        reason: impl Into<Cow<'static, str>>,
        mutate: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit,
    {
        let (result, notification) = {
            let mut registry = self.registry.lock().expect("store poisoned");
            registry.ensure::<T>(&self.resources.get());
            let result = mutate(registry.get_mut::<T>().expect("lazy store must exist"));
            (
                result,
                StoreNotification {
                    store_key: T::KEY,
                    reason: reason.into(),
                },
            )
        };
        self.dispatch_notification(notification);
        result
    }

    pub fn update_defined<T, R>(
        &self,
        definition: StoreDefinition<T>,
        reason: impl Into<Cow<'static, str>>,
        mutate: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: Send + Sync + 'static,
    {
        let (result, notification) = {
            let mut registry = self.registry.lock().expect("store poisoned");
            registry.ensure_defined(definition);
            let result = mutate(
                registry
                    .get_mut::<T>()
                    .expect("lazy defined store must exist"),
            );
            (
                result,
                StoreNotification {
                    store_key: definition.key(),
                    reason: reason.into(),
                },
            )
        };
        self.dispatch_notification(notification);
        result
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

    fn dispatch_notification(&self, notification: StoreNotification) {
        let observers = self
            .registry
            .lock()
            .expect("store poisoned")
            .observers_for(&notification);
        for observer in observers {
            observer.on_store_notification(&notification);
        }
        if let Some(wake) = self.wake.read().expect("store wake lock poisoned").as_ref() {
            wake();
        }
    }
}
