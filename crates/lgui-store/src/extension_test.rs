use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use lgui_core::{
    application::ApplicationContext, memory::test_memory_options, resources::Resources,
};

use super::StoreApplicationExt;
use crate::StoreUnit;

struct CounterResource(Arc<AtomicUsize>);

struct LazyStore;

impl StoreUnit for LazyStore {
    const KEY: &'static str = "test.lazy";

    fn create(resources: &Resources) -> Self {
        resources
            .require::<CounterResource>()
            .0
            .fetch_add(1, Ordering::SeqCst);
        Self
    }
}

struct ValueStore(i32);

impl StoreUnit for ValueStore {
    const KEY: &'static str = "test.value";

    fn create(_resources: &Resources) -> Self {
        Self(0)
    }
}

struct DroppedStore(Arc<AtomicUsize>);

impl Drop for DroppedStore {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl StoreUnit for DroppedStore {
    const KEY: &'static str = "test.drop";

    fn create(resources: &Resources) -> Self {
        Self(resources.require::<CounterResource>().0.clone())
    }
}

fn context_with_counter(counter: Arc<AtomicUsize>) -> ApplicationContext {
    let context = ApplicationContext::empty(test_memory_options());
    context.resources().provide(CounterResource(counter));
    context
}

#[test]
fn stores_are_created_once_on_first_use() {
    let creations = Arc::new(AtomicUsize::new(0));
    let application = context_with_counter(Arc::clone(&creations));

    assert_eq!(creations.load(Ordering::SeqCst), 0);
    application.read_store::<LazyStore, _>(|_| ());
    application.read_store::<LazyStore, _>(|_| ());
    assert_eq!(creations.load(Ordering::SeqCst), 1);
}

#[test]
fn store_instances_are_isolated_per_application() {
    let first = ApplicationContext::empty(test_memory_options());
    let second = ApplicationContext::empty(test_memory_options());

    first.update_store::<ValueStore, _>("first", |store| store.0 = 7);

    assert_eq!(first.read_store::<ValueStore, _>(|store| store.0), 7);
    assert_eq!(second.read_store::<ValueStore, _>(|store| store.0), 0);
}

#[test]
fn store_updates_wake_the_owning_application() {
    let application = ApplicationContext::empty(test_memory_options());
    let wakes = Arc::new(AtomicUsize::new(0));
    application.stores().set_wake({
        let wakes = Arc::clone(&wakes);
        Arc::new(move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        })
    });

    application.update_store::<ValueStore, _>("wake", |store| store.0 += 1);

    assert_eq!(wakes.load(Ordering::SeqCst), 1);
}

#[test]
fn application_drop_releases_its_store_instances() {
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let application = context_with_counter(Arc::clone(&drops));
        application.read_store::<DroppedStore, _>(|_| ());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
    }
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
