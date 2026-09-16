use std::{future::Future, pin::Pin, sync::Arc};

#[cfg(feature = "async")]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    task::{Context, Poll, Waker},
};

pub type UiTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub trait UiExecutor: Send + Sync + 'static {
    fn spawn(&self, task: UiTask);
}

impl<F> UiExecutor for F
where
    F: Fn(UiTask) + Send + Sync + 'static,
{
    fn spawn(&self, task: UiTask) {
        self(task);
    }
}

pub type UiTaskSpawner = Arc<dyn UiExecutor>;

pub fn noop_task_spawner() -> UiTaskSpawner {
    Arc::new(|_| {})
}

#[cfg(feature = "tokio")]
#[derive(Clone)]
pub struct TokioExecutor {
    handle: tokio::runtime::Handle,
}

#[cfg(feature = "tokio")]
impl TokioExecutor {
    pub fn new(handle: tokio::runtime::Handle) -> Self {
        Self { handle }
    }

    pub fn current() -> Self {
        Self::new(tokio::runtime::Handle::current())
    }
}

#[cfg(feature = "tokio")]
impl UiExecutor for TokioExecutor {
    fn spawn(&self, task: UiTask) {
        self.handle.spawn(task);
    }
}

#[cfg(feature = "async")]
#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

#[cfg(feature = "async")]
pub(crate) struct UiTaskCancellation {
    state: Arc<CancellationState>,
}

#[cfg(feature = "async")]
impl UiTaskCancellation {
    pub(crate) fn cancel(self) {
        self.state.cancelled.store(true, Ordering::Release);
        if let Some(waker) = self
            .state
            .waker
            .lock()
            .expect("task cancellation waker poisoned")
            .take()
        {
            waker.wake();
        }
    }
}

#[cfg(feature = "async")]
struct CancellationFuture {
    state: Arc<CancellationState>,
}

#[cfg(feature = "async")]
impl Future for CancellationFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.state.cancelled.load(Ordering::Acquire) {
            return Poll::Ready(());
        }

        let mut waker = self
            .state
            .waker
            .lock()
            .expect("task cancellation waker poisoned");
        if self.state.cancelled.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        if waker
            .as_ref()
            .is_none_or(|registered| !registered.will_wake(cx.waker()))
        {
            *waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}

#[cfg(feature = "async")]
struct CancellableTask {
    task: UiTask,
    cancelled: CancellationFuture,
}

#[cfg(feature = "async")]
impl Future for CancellableTask {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.task.as_mut().poll(cx).is_ready() {
            return Poll::Ready(());
        }
        Pin::new(&mut self.cancelled).poll(cx)
    }
}

#[cfg(feature = "async")]
pub(crate) fn cancellable_task(task: UiTask) -> (UiTaskCancellation, UiTask) {
    let state = Arc::new(CancellationState::default());
    (
        UiTaskCancellation {
            state: Arc::clone(&state),
        },
        Box::pin(CancellableTask {
            task,
            cancelled: CancellationFuture { state },
        }),
    )
}

#[cfg(all(test, feature = "async"))]
#[path = "task_test.rs"]
mod tests;
