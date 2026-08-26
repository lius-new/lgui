use std::sync::{
    atomic::{AtomicBool, AtomicIsize, Ordering},
    Arc, Mutex,
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, WM_APP},
};

use crate::application::{ApplicationHandle, ApplicationTask};

pub const WM_LGUI_DISPATCH: u32 = WM_APP + 44;

#[derive(Clone, Default)]
pub struct Win32Dispatcher {
    inner: Arc<DispatcherInner>,
}

#[derive(Default)]
struct DispatcherInner {
    window: AtomicIsize,
    frame_requested: AtomicBool,
    tasks: Mutex<Vec<ApplicationTask>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Win32DispatchResult {
    pub tasks_executed: bool,
    pub frame_requested: bool,
}

impl Win32Dispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach(&self, hwnd: HWND) {
        self.inner.window.store(hwnd.0 as isize, Ordering::Release);
    }

    pub fn detach(&self, hwnd: HWND) {
        let _ = self.inner.window.compare_exchange(
            hwnd.0 as isize,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn window(&self) -> Option<HWND> {
        let hwnd = self.inner.window.load(Ordering::Acquire);
        (hwnd != 0).then_some(HWND(hwnd as _))
    }

    pub fn application_handle(&self) -> ApplicationHandle {
        let post = self.clone();
        let frame = self.clone();
        ApplicationHandle::new(
            move |task| post.post_boxed(task),
            move || frame.request_frame(),
        )
    }

    pub fn post(&self, task: impl FnOnce() + Send + 'static) {
        self.post_boxed(Box::new(task));
    }

    pub fn request_frame(&self) {
        self.inner.frame_requested.store(true, Ordering::Release);
        self.wake();
    }

    pub fn drain(&self) -> Win32DispatchResult {
        let tasks = std::mem::take(
            &mut *self
                .inner
                .tasks
                .lock()
                .expect("UI thread task queue poisoned"),
        );
        let tasks_executed = !tasks.is_empty();
        for task in tasks {
            task();
        }
        Win32DispatchResult {
            tasks_executed,
            frame_requested: self.inner.frame_requested.swap(false, Ordering::AcqRel),
        }
    }

    fn post_boxed(&self, task: ApplicationTask) {
        self.inner
            .tasks
            .lock()
            .expect("UI thread task queue poisoned")
            .push(task);
        self.wake();
    }

    fn wake(&self) {
        let hwnd = self.inner.window.load(Ordering::Acquire);
        if hwnd == 0 {
            return;
        }
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as _)),
                WM_LGUI_DISPATCH,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
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
}
