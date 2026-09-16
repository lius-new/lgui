use std::sync::{
    atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, WM_APP},
};

use lgui_core::application::{ApplicationHandle, ApplicationTask};

pub const WM_LGUI_DISPATCH: u32 = WM_APP + 44;
pub const WM_LGUI_FRAME_TICK: u32 = WM_APP + 45;

pub(super) const DEFAULT_FRAME_INTERVAL_MS: u64 = 16;

#[derive(Clone, Default)]
pub struct Win32Dispatcher {
    inner: Arc<DispatcherInner>,
}

#[derive(Clone)]
pub(super) struct CoalescedTrim {
    inner: Arc<CoalescedTrimInner>,
}

struct CoalescedTrimInner {
    dispatcher: Win32Dispatcher,
    pending: Mutex<PendingTrim>,
    trim: Arc<dyn Fn(usize) -> usize + Send + Sync>,
}

#[derive(Default)]
struct PendingTrim {
    target_bytes: Option<usize>,
    scheduled: bool,
}

#[derive(Default)]
struct DispatcherInner {
    window: AtomicIsize,
    frame_requested: AtomicBool,
    frame_driver_running: AtomicBool,
    frame_tick_pending: AtomicBool,
    frame_generation: AtomicU64,
    frame_interval_ms: AtomicU64,
    last_frame_tick: Mutex<Option<Instant>>,
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

    pub fn start_frame_driver(&self) {
        if self
            .inner
            .frame_driver_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let generation = self.inner.frame_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.inner
            .frame_interval_ms
            .store(DEFAULT_FRAME_INTERVAL_MS, Ordering::Release);
        *self
            .inner
            .last_frame_tick
            .lock()
            .expect("frame clock poisoned") = Some(Instant::now());

        let driver = self.clone();
        if std::thread::Builder::new()
            .name("lgui-animation".to_owned())
            .spawn(move || driver.run_frame_driver(generation))
            .is_err()
        {
            self.stop_frame_driver();
        }
    }

    pub fn frame_elapsed_ms(&self) -> f32 {
        let now = Instant::now();
        let mut last = self
            .inner
            .last_frame_tick
            .lock()
            .expect("frame clock poisoned");
        last.replace(now)
            .map(|previous| now.duration_since(previous).as_secs_f32() * 1000.0)
            .unwrap_or(self.frame_interval().as_secs_f32() * 1000.0)
            .clamp(1.0, 50.0)
    }

    pub fn finish_frame_tick(&self, should_continue: bool) {
        self.finish_frame_tick_with_interval(should_continue, DEFAULT_FRAME_INTERVAL_MS);
    }

    pub fn finish_frame_tick_with_interval(&self, should_continue: bool, interval_ms: u64) {
        if should_continue {
            self.inner
                .frame_interval_ms
                .store(interval_ms.max(1), Ordering::Release);
            self.inner
                .frame_tick_pending
                .store(false, Ordering::Release);
        } else {
            self.stop_frame_driver();
        }
    }

    pub fn stop_frame_driver(&self) {
        self.inner
            .frame_driver_running
            .store(false, Ordering::Release);
        self.inner
            .frame_tick_pending
            .store(false, Ordering::Release);
        self.inner.frame_generation.fetch_add(1, Ordering::AcqRel);
        *self
            .inner
            .last_frame_tick
            .lock()
            .expect("frame clock poisoned") = None;
    }

    /// Wakes the UI thread without declaring that every window needs a frame.
    ///
    /// Retained state/store updates use this path because the affected component queues carry
    /// their own invalidation identities.
    pub fn notify(&self) {
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

    fn run_frame_driver(&self, generation: u64) {
        while self.inner.frame_driver_running.load(Ordering::Acquire)
            && self.inner.frame_generation.load(Ordering::Acquire) == generation
        {
            std::thread::sleep(self.frame_interval());
            if !self.inner.frame_driver_running.load(Ordering::Acquire)
                || self.inner.frame_generation.load(Ordering::Acquire) != generation
            {
                break;
            }
            if self
                .inner
                .frame_tick_pending
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            if !self.post_message(WM_LGUI_FRAME_TICK) {
                self.inner
                    .frame_tick_pending
                    .store(false, Ordering::Release);
            }
        }
    }

    fn frame_interval(&self) -> Duration {
        let interval_ms = self.inner.frame_interval_ms.load(Ordering::Acquire);
        Duration::from_millis(if interval_ms == 0 {
            DEFAULT_FRAME_INTERVAL_MS
        } else {
            interval_ms
        })
    }

    fn wake(&self) {
        let _ = self.post_message(WM_LGUI_DISPATCH);
    }

    fn post_message(&self, message: u32) -> bool {
        let hwnd = self.inner.window.load(Ordering::Acquire);
        if hwnd == 0 {
            return false;
        }
        unsafe { PostMessageW(Some(HWND(hwnd as _)), message, WPARAM(0), LPARAM(0)).is_ok() }
    }
}

impl CoalescedTrim {
    pub(super) fn new(
        dispatcher: Win32Dispatcher,
        trim: impl Fn(usize) -> usize + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(CoalescedTrimInner {
                dispatcher,
                pending: Mutex::new(PendingTrim::default()),
                trim: Arc::new(trim),
            }),
        }
    }

    pub(super) fn request(&self, target_bytes: usize) {
        let should_schedule = {
            let mut pending = self
                .inner
                .pending
                .lock()
                .expect("coalesced trim state poisoned");
            pending.target_bytes = Some(
                pending
                    .target_bytes
                    .map_or(target_bytes, |current| current.min(target_bytes)),
            );
            if pending.scheduled {
                false
            } else {
                pending.scheduled = true;
                true
            }
        };
        if should_schedule {
            let pending = self.clone();
            self.inner.dispatcher.post(move || pending.drain());
        }
    }

    fn drain(&self) {
        loop {
            let target_bytes = {
                let mut pending = self
                    .inner
                    .pending
                    .lock()
                    .expect("coalesced trim state poisoned");
                match pending.target_bytes.take() {
                    Some(target_bytes) => target_bytes,
                    None => {
                        pending.scheduled = false;
                        return;
                    }
                }
            };
            (self.inner.trim)(target_bytes);
        }
    }
}

#[cfg(test)]
#[path = "dispatcher_test.rs"]
mod tests;
