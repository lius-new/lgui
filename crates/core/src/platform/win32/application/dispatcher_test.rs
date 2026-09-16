use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use super::*;

#[test]
fn dispatcher_marshals_cross_thread_tasks_and_coalesces_frame_requests() {
    let dispatcher = Win32Dispatcher::new();
    let completed = Arc::new(AtomicUsize::new(0));
    let worker_dispatcher = dispatcher.clone();
    let worker_completed = Arc::clone(&completed);

    std::thread::spawn(move || {
        worker_dispatcher.post(move || {
            worker_completed.fetch_add(1, Ordering::SeqCst);
        });
        worker_dispatcher.request_frame();
        worker_dispatcher.request_frame();
    })
    .join()
    .expect("dispatcher producer thread should finish");

    assert_eq!(completed.load(Ordering::SeqCst), 0);
    let drained = dispatcher.drain();
    assert!(drained.tasks_executed);
    assert!(drained.frame_requested);
    assert_eq!(completed.load(Ordering::SeqCst), 1);
    assert_eq!(dispatcher.drain(), Win32DispatchResult::default());
}

#[test]
fn frame_driver_accepts_a_component_requested_interval() {
    let dispatcher = Win32Dispatcher::new();

    dispatcher.finish_frame_tick_with_interval(true, 33);

    assert_eq!(dispatcher.frame_interval(), Duration::from_millis(33));
    dispatcher.stop_frame_driver();
}

#[test]
fn coalesced_trim_keeps_the_strictest_pending_target_and_can_be_reused() {
    let dispatcher = Win32Dispatcher::new();
    let targets = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&targets);
    let trim = CoalescedTrim::new(dispatcher.clone(), move |target_bytes| {
        observed
            .lock()
            .expect("trim targets poisoned")
            .push(target_bytes);
        0
    });

    trim.request(64);
    trim.request(32);
    trim.request(48);
    dispatcher.drain();
    assert_eq!(*targets.lock().expect("trim targets poisoned"), vec![32]);

    trim.request(40);
    dispatcher.drain();
    assert_eq!(
        *targets.lock().expect("trim targets poisoned"),
        vec![32, 40]
    );
}
