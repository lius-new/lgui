use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

use crate::{
    application::ApplicationContext,
    core::{UiTask, UiTaskSpawner},
    window::WindowId,
};

use super::*;

#[test]
fn async_handler_spawns_with_an_owned_ui_context() {
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    application.set_executor(Arc::new(|mut task: UiTask| {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(task.as_mut().poll(&mut context), Poll::Ready(())));
    }) as UiTaskSpawner);
    let calls = Arc::new(AtomicUsize::new(0));
    let handler = async_handler({
        let calls = Arc::clone(&calls);
        move |context| {
            let calls = Arc::clone(&calls);
            async move {
                assert_eq!(context.window().unwrap().id().as_str(), "async-handler");
                calls.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let mut context = UiEventContext::new(application, WindowId::new("async-handler"));

    handler(&mut context);

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(context.flags().consumed);
}

#[test]
fn async_handler_with_moves_the_control_argument_into_the_task() {
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    application.set_executor(Arc::new(|mut task: UiTask| {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(task.as_mut().poll(&mut context), Poll::Ready(())));
    }) as UiTaskSpawner);
    let total = Arc::new(AtomicUsize::new(0));
    let handler = async_handler_with({
        let total = Arc::clone(&total);
        move |_context, amount| {
            let total = Arc::clone(&total);
            async move {
                total.fetch_add(amount, Ordering::SeqCst);
            }
        }
    });
    let mut context = UiEventContext::new(application, WindowId::new("async-handler-with"));

    handler(&mut context, 3);

    assert_eq!(total.load(Ordering::SeqCst), 3);
    assert!(context.flags().consumed);
}
