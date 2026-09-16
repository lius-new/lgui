use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

use super::*;

#[derive(Clone)]
struct CounterEvent(usize);

const COUNTER: EventKey<CounterEvent> = EventKey::new("test.counter");

fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

impl Event for CounterEvent {
    const NAME: &'static str = "test.counter";
}

#[test]
fn events_are_typed_and_subscriptions_stop_on_drop() {
    let bus = EventBus::default();
    let total = Arc::new(AtomicUsize::new(0));
    let subscription = bus.subscribe(COUNTER.clone(), {
        let total = Arc::clone(&total);
        move |event| {
            total.fetch_add(event.0, Ordering::SeqCst);
        }
    });

    assert_eq!(bus.emit(&COUNTER, CounterEvent(3)), 1);
    assert_eq!(total.load(Ordering::SeqCst), 3);
    drop(subscription);
    assert_eq!(bus.emit(&COUNTER, CounterEvent(4)), 0);
    assert_eq!(total.load(Ordering::SeqCst), 3);
}

#[test]
fn event_buses_are_isolated() {
    let first = EventBus::default();
    let second = EventBus::default();
    let deliveries = Arc::new(AtomicUsize::new(0));
    let _subscription = first.subscribe(COUNTER.clone(), {
        let deliveries = Arc::clone(&deliveries);
        move |_| {
            deliveries.fetch_add(1, Ordering::SeqCst);
        }
    });

    assert_eq!(second.emit(&COUNTER, CounterEvent(1)), 0);
    assert_eq!(deliveries.load(Ordering::SeqCst), 0);
}

#[test]
fn dynamic_and_static_keys_with_the_same_name_share_a_topic() {
    let bus = EventBus::default();
    let dynamic = EventKey::dynamic(Arc::<str>::from("test.counter")).unwrap();
    let total = Arc::new(AtomicUsize::new(0));
    let _subscription = bus.subscribe(COUNTER.clone(), {
        let total = Arc::clone(&total);
        move |event| {
            total.fetch_add(event.0, Ordering::SeqCst);
        }
    });

    assert_eq!(bus.emit(&dynamic, CounterEvent(5)), 1);
    assert_eq!(total.load(Ordering::SeqCst), 5);
}

#[test]
fn emitting_an_unobserved_dynamic_key_does_not_create_a_topic() {
    let bus = EventBus::default();
    let key = EventKey::dynamic(Arc::<str>::from("server.unobserved")).unwrap();

    assert_eq!(bus.emit(&key, CounterEvent(1)), 0);
    assert_eq!(bus.topic_count(), 0);
}

#[test]
#[should_panic(expected = "already bound to payload type")]
fn a_key_name_cannot_be_reused_with_a_different_payload_type() {
    let bus = EventBus::default();
    let _subscription = bus.subscribe(COUNTER.clone(), |_| {});
    let conflicting = EventKey::<String>::new("test.counter");

    let _ = bus.subscribe(conflicting, |_| {});
}

#[test]
fn invalid_dynamic_keys_are_rejected() {
    assert!(EventKey::<CounterEvent>::dynamic("Uppercase.Key").is_err());
    assert!(EventKey::<CounterEvent>::dynamic("").is_err());
}

#[test]
fn free_emit_routes_through_the_scoped_application() {
    let application =
        crate::application::ApplicationContext::empty(crate::memory::test_memory_options());
    let total = Arc::new(AtomicUsize::new(0));
    let _subscription = application.subscribe_keyed(COUNTER.clone(), {
        let total = Arc::clone(&total);
        move |event| {
            total.fetch_add(event.0, Ordering::SeqCst);
        }
    });

    assert_eq!(
        run_ready(application.scope(emit(COUNTER.clone(), CounterEvent(7)))),
        1
    );
    assert_eq!(total.load(Ordering::SeqCst), 7);
}
