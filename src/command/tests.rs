use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

use crate::application::ApplicationContext;

use super::*;

struct Add;

impl Command for Add {
    type Args = (i32, i32);
    type Output = i32;
    type Error = &'static str;

    const NAME: &'static str = "test.add";
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

#[test]
fn typed_commands_preserve_arguments_outputs_and_errors() {
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    assert!(application
        .command_registry()
        .register::<Add>(|_, (left, right)| async move {
            if left < 0 {
                Err("negative")
            } else {
                Ok(left + right)
            }
        }));

    let command = application.command::<Add>();
    assert_eq!(run_ready(command.invoke((2, 3))), Ok(5));
    assert_eq!(run_ready(command.invoke((-1, 3))), Err("negative"));
}

#[test]
fn duplicate_command_registration_is_rejected() {
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    assert!(application
        .command_registry()
        .register::<Add>(|_, _| async { Ok(0) }));
    assert!(!application
        .command_registry()
        .register::<Add>(|_, _| async { Ok(1) }));
}
