use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use super::*;

#[derive(Clone)]
struct CounterEvent(usize);

impl Event for CounterEvent {
    const NAME: &'static str = "test.counter";
}

#[test]
fn events_are_typed_and_subscriptions_stop_on_drop() {
    let bus = EventBus::default();
    let total = Arc::new(AtomicUsize::new(0));
    let subscription = bus.subscribe::<CounterEvent>({
        let total = Arc::clone(&total);
        move |event| {
            total.fetch_add(event.0, Ordering::SeqCst);
        }
    });

    assert_eq!(bus.emit(CounterEvent(3)), 1);
    assert_eq!(total.load(Ordering::SeqCst), 3);
    drop(subscription);
    assert_eq!(bus.emit(CounterEvent(4)), 0);
    assert_eq!(total.load(Ordering::SeqCst), 3);
}

#[test]
fn event_buses_are_isolated() {
    let first = EventBus::default();
    let second = EventBus::default();
    let deliveries = Arc::new(AtomicUsize::new(0));
    let _subscription = first.subscribe::<CounterEvent>({
        let deliveries = Arc::clone(&deliveries);
        move |_| {
            deliveries.fetch_add(1, Ordering::SeqCst);
        }
    });

    assert_eq!(second.emit(CounterEvent(1)), 0);
    assert_eq!(deliveries.load(Ordering::SeqCst), 0);
}
