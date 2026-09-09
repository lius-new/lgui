use std::{
    future::{poll_fn, Future},
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

use super::*;

#[test]
fn scope_is_active_only_while_the_future_is_polled() {
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut first_poll = true;
    let future = application.scope({
        let application = application.clone();
        let observed = Arc::clone(&observed);
        async move {
            poll_fn(move |cx| {
                observed
                    .lock()
                    .expect("scope observations poisoned")
                    .push(try_current_application().as_ref() == Some(&application));
                if first_poll {
                    first_poll = false;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
        }
    });

    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        if Future::poll(future.as_mut(), &mut context).is_ready() {
            break;
        }
    }

    assert_eq!(
        observed
            .lock()
            .expect("scope observations poisoned")
            .as_slice(),
        &[true, true]
    );
    assert!(try_current_application().is_none());
}
