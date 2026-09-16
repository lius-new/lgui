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
#[path = "scope_test.rs"]
mod tests;
