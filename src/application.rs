use std::sync::Arc;

use crate::core::{Element, RenderCx, RootComponent, Size};

pub type AppView = Arc<
    dyn for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
>;

pub type ApplicationTask = Box<dyn FnOnce() + Send + 'static>;

/// Cloneable, platform-neutral access to the running application event loop.
#[derive(Clone)]
pub struct ApplicationHandle {
    post: Arc<dyn Fn(ApplicationTask) + Send + Sync>,
    request_frame: Arc<dyn Fn() + Send + Sync>,
}

impl ApplicationHandle {
    pub fn new(
        post: impl Fn(ApplicationTask) + Send + Sync + 'static,
        request_frame: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            post: Arc::new(post),
            request_frame: Arc::new(request_frame),
        }
    }

    pub fn post(&self, task: impl FnOnce() + Send + 'static) {
        (self.post)(Box::new(task));
    }

    pub fn request_frame(&self) {
        (self.request_frame)();
    }
}

impl RootComponent for AppView {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> Element {
        self(cx)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    pub title: String,
    pub size: Size,
    pub minimum_size: Option<Size>,
    pub resizable: bool,
}

impl WindowOptions {
    pub fn new(title: impl Into<String>, size: Size) -> Self {
        Self {
            title: title.into(),
            size,
            ..Self::default()
        }
    }

    pub fn minimum_size(mut self, size: Size) -> Self {
        self.minimum_size = Some(size);
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "lgui".to_owned(),
            size: Size::new(1024, 720),
            minimum_size: None,
            resizable: true,
        }
    }
}

pub trait ApplicationBackend: Sized {
    type Error;

    fn run(self, options: WindowOptions, view: AppView) -> Result<(), Self::Error>;
}

pub struct Application<B> {
    backend: B,
    window: WindowOptions,
}

impl<B> Application<B> {
    pub fn with_backend(backend: B) -> Self {
        Self {
            backend,
            window: WindowOptions::default(),
        }
    }

    pub fn window_options(mut self, options: WindowOptions) -> Self {
        self.window = options;
        self
    }
}

#[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
impl Application<crate::platform::win32::Win32Application> {
    pub fn new() -> Self {
        Self::with_backend(crate::platform::win32::Win32Application::default())
    }
}

impl<B> Application<B>
where
    B: ApplicationBackend,
{
    pub fn run(
        self,
        view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
            + Send
            + Sync
            + 'static,
    ) -> Result<(), B::Error> {
        self.backend.run(self.window, Arc::new(view))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    struct RecordingBackend(Arc<Mutex<Option<WindowOptions>>>);

    impl ApplicationBackend for RecordingBackend {
        type Error = ();

        fn run(self, options: WindowOptions, _view: AppView) -> Result<(), Self::Error> {
            *self.0.lock().expect("window options lock poisoned") = Some(options);
            Ok(())
        }
    }

    #[test]
    fn application_passes_window_options_to_the_backend() {
        let recorded = Arc::new(Mutex::new(None));
        let options = WindowOptions::new("counter", Size::new(640, 480))
            .minimum_size(Size::new(320, 240))
            .resizable(false);

        Application::with_backend(RecordingBackend(Arc::clone(&recorded)))
            .window_options(options.clone())
            .run(|_| crate::core::group(crate::core::UiRect::new(0, 0, 1, 1)))
            .expect("mock backend should run");

        assert_eq!(
            recorded
                .lock()
                .expect("window options lock poisoned")
                .clone(),
            Some(options)
        );
    }

    #[test]
    fn application_handle_posts_tasks_and_requests_frames() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let posted = Arc::new(AtomicUsize::new(0));
        let frames = Arc::new(AtomicUsize::new(0));
        let handle = ApplicationHandle::new(
            {
                let posted = Arc::clone(&posted);
                move |task| {
                    task();
                    posted.fetch_add(1, Ordering::SeqCst);
                }
            },
            {
                let frames = Arc::clone(&frames);
                move || {
                    frames.fetch_add(1, Ordering::SeqCst);
                }
            },
        );

        handle.post(|| {});
        handle.request_frame();

        assert_eq!(posted.load(Ordering::SeqCst), 1);
        assert_eq!(frames.load(Ordering::SeqCst), 1);
    }
}
