use std::{
    future::pending,
    task::{Context, Poll, Waker},
};

use super::*;

#[test]
fn cancellation_completes_a_pending_task_without_an_executor_dependency() {
    let (cancel, mut task) = cancellable_task(Box::pin(pending()));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(matches!(task.as_mut().poll(&mut context), Poll::Pending));
    cancel.cancel();
    assert!(matches!(task.as_mut().poll(&mut context), Poll::Ready(())));
}
