use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use super::ApplicationContext;

thread_local! {
    static CURRENT_APPLICATION: RefCell<Vec<ApplicationContext>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct ApplicationScopeFuture<F> {
    application: ApplicationContext,
    future: Pin<Box<F>>,
}

impl<F> ApplicationScopeFuture<F> {
    pub(crate) fn new(application: ApplicationContext, future: F) -> Self {
        Self {
            application,
            future: Box::pin(future),
        }
    }
}

impl<F> Future for ApplicationScopeFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let _guard = enter(this.application.clone());
        this.future.as_mut().poll(cx)
    }
}

struct ApplicationScopeGuard;

fn enter(application: ApplicationContext) -> ApplicationScopeGuard {
    CURRENT_APPLICATION.with(|current| current.borrow_mut().push(application));
    ApplicationScopeGuard
}

pub(crate) fn try_current_application() -> Option<ApplicationContext> {
    CURRENT_APPLICATION.with(|current| current.borrow().last().cloned())
}

pub(crate) fn current_application(operation: &str) -> ApplicationContext {
    try_current_application().unwrap_or_else(|| {
        panic!(
            "`{operation}` requires an active LGUI Application scope; run the future through ApplicationContext::scope or an LGUI task API"
        )
    })
}

impl Drop for ApplicationScopeGuard {
    fn drop(&mut self) {
        CURRENT_APPLICATION.with(|current| {
            current
                .borrow_mut()
                .pop()
                .expect("LGUI Application scope stack underflow");
        });
    }
}

#[cfg(test)]
mod tests {
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
}
