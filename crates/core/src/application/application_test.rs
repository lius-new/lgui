use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
};

use crate::{
    application::RenderErrorStage,
    command::Command,
    core::{Size, UiRect},
};

#[cfg(feature = "store")]
use crate::{command::CommandRegistry, events::EventBus};

use super::*;

struct RecordingBackend(Arc<Mutex<Option<WindowOptions>>>);

struct ReportingBackend;

struct BuilderCommand;

impl Command for BuilderCommand {
    type Args = usize;
    type Output = usize;
    type Error = ();

    const NAME: &'static str = "test.builder";
}

struct InvokingBackend(Arc<Mutex<Option<usize>>>);

impl ApplicationBackend for RecordingBackend {
    type Error = ();

    fn run(
        self,
        options: WindowOptions,
        _view: AppView,
        _context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        *self.0.lock().expect("window options lock poisoned") = Some(options);
        Ok(())
    }
}

impl ApplicationBackend for ReportingBackend {
    type Error = ();

    fn run(
        self,
        _options: WindowOptions,
        _view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        context.report_render_error(RenderError::new(
            WindowId::new("reporting-window"),
            "test",
            RenderErrorStage::Present,
            "present_test_frame",
            0x80004005_u32 as i32,
            "synthetic present failure",
        ));
        Ok(())
    }
}

impl ApplicationBackend for InvokingBackend {
    type Error = ();

    fn run(
        self,
        _options: WindowOptions,
        _view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        let mut future = Box::pin(context.invoke::<BuilderCommand>(41));
        let waker = Waker::noop();
        let mut task_context = Context::from_waker(waker);
        let Poll::Ready(result) = future.as_mut().poll(&mut task_context) else {
            panic!("test command unexpectedly pending");
        };
        *self.0.lock().expect("command result lock poisoned") = Some(result?);
        Ok(())
    }
}

#[test]
fn application_builder_registers_commands_on_the_runtime_context() {
    let result = Arc::new(Mutex::new(None));

    Application::with_backend(InvokingBackend(Arc::clone(&result)))
        .memory_options(crate::memory::test_memory_options())
        .command::<BuilderCommand>(|_, value| async move { Ok(value + 1) })
        .run(|cx| crate::core::group(cx.viewport()))
        .expect("invoking backend should run");

    assert_eq!(
        *result.lock().expect("command result lock poisoned"),
        Some(42)
    );
}

#[test]
fn application_memory_options_are_owned_by_the_runtime_context() {
    struct MemoryBackend(Arc<Mutex<Option<crate::memory::MemoryOptions>>>);

    impl ApplicationBackend for MemoryBackend {
        type Error = ();

        fn run(
            self,
            _options: WindowOptions,
            _view: AppView,
            context: ApplicationContext,
        ) -> Result<(), Self::Error> {
            *self.0.lock().expect("memory options lock poisoned") =
                Some(context.memory().options());
            Ok(())
        }
    }

    let captured = Arc::new(Mutex::new(None));
    let options = crate::memory::test_memory_options();
    Application::with_backend(MemoryBackend(Arc::clone(&captured)))
        .memory_options(options)
        .run(|cx| crate::core::group(cx.viewport()))
        .unwrap();

    assert_eq!(
        *captured.lock().expect("memory options lock poisoned"),
        Some(options)
    );
}

#[test]
fn render_errors_are_delivered_with_structured_context() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&errors);

    Application::with_backend(ReportingBackend)
        .memory_options(crate::memory::test_memory_options())
        .on_render_error(move |error| {
            captured
                .lock()
                .expect("render error capture poisoned")
                .push(error.clone());
        })
        .run(|cx| crate::core::group(cx.viewport()))
        .expect("reporting backend should run");

    let errors = errors.lock().expect("render error capture poisoned");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].window().as_str(), "reporting-window");
    assert_eq!(errors[0].renderer(), "test");
    assert_eq!(errors[0].stage(), RenderErrorStage::Present);
    assert_eq!(errors[0].operation(), "present_test_frame");
    assert_eq!(errors[0].code(), 0x80004005_u32 as i32);
    assert_eq!(errors[0].message(), "synthetic present failure");
}

#[test]
fn application_passes_window_options_to_the_backend() {
    let recorded = Arc::new(Mutex::new(None));
    let options = WindowOptions::new("counter")
        .size(Size::new(640.0, 480.0))
        .minimum_size(Size::new(320.0, 240.0))
        .maximum_size(Size::new(1280.0, 960.0))
        .resizable(false);

    Application::with_backend(RecordingBackend(Arc::clone(&recorded)))
        .memory_options(crate::memory::test_memory_options())
        .window_options(options.clone())
        .run(|_| crate::core::group(UiRect::new(0.0, 0.0, 1.0, 1.0)))
        .expect("mock backend should run");

    let recorded = recorded
        .lock()
        .expect("window options lock poisoned")
        .clone()
        .expect("backend should receive window options");
    assert_eq!(recorded.id, options.id);
    assert_eq!(recorded.size, options.size);
    assert_eq!(recorded.minimum_size, options.minimum_size);
    assert_eq!(recorded.maximum_size, options.maximum_size);
    assert_eq!(recorded.resizable, options.resizable);
}

#[test]
fn application_handle_posts_tasks_and_requests_frames() {
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

struct RenderingBackend(Arc<AtomicUsize>);

impl ApplicationBackend for RenderingBackend {
    type Error = ();

    fn run(
        self,
        _options: WindowOptions,
        view: AppView,
        _context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        let mut session = crate::session::UiSession::new();
        let _ = session.render_view(
            &view,
            UiRect::new(0.0, 0.0, 320.0, 200.0),
            crate::core::UiScale::ONE,
        );
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn application_run_mounts_and_renders_the_root_component() {
    let backend_frames = Arc::new(AtomicUsize::new(0));
    let root_executions = Arc::new(AtomicUsize::new(0));
    let executions = Arc::clone(&root_executions);

    Application::with_backend(RenderingBackend(Arc::clone(&backend_frames)))
        .memory_options(crate::memory::test_memory_options())
        .run(move |cx| {
            let _application = cx.application();
            executions.fetch_add(1, Ordering::SeqCst);
            crate::core::content_text("root")
        })
        .expect("recording backend should render");

    assert_eq!(backend_frames.load(Ordering::SeqCst), 1);
    assert_eq!(root_executions.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "store")]
mod store_lifecycle {
    use super::*;
    use crate::{resources::Resources, store::StoreUnit};

    struct CreateCounter(Arc<AtomicUsize>);

    struct LazyStore;

    impl StoreUnit for LazyStore {
        const KEY: &'static str = "test.lazy";

        fn create(resources: &Resources) -> Self {
            resources
                .require::<CreateCounter>()
                .0
                .fetch_add(1, Ordering::SeqCst);
            Self
        }
    }

    struct ValueStore(i32);

    impl StoreUnit for ValueStore {
        const KEY: &'static str = "test.value";

        fn create(_resources: &Resources) -> Self {
            Self(0)
        }
    }

    struct DroppedStore(Arc<AtomicUsize>);

    impl Drop for DroppedStore {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl StoreUnit for DroppedStore {
        const KEY: &'static str = "test.drop";

        fn create(resources: &Resources) -> Self {
            Self(resources.require::<CreateCounter>().0.clone())
        }
    }

    fn context_with_counter(counter: Arc<AtomicUsize>) -> ApplicationContext {
        let resources = Resources::new();
        resources.provide(CreateCounter(counter));
        ApplicationContext::new(
            resources,
            None,
            CommandRegistry::default(),
            EventBus::default(),
            crate::memory::test_memory_options(),
        )
    }

    #[test]
    fn stores_are_created_once_on_first_use() {
        let creations = Arc::new(AtomicUsize::new(0));
        let application = context_with_counter(Arc::clone(&creations));

        assert_eq!(creations.load(Ordering::SeqCst), 0);
        application.read_store::<LazyStore, _>(|_| ());
        application.read_store::<LazyStore, _>(|_| ());
        assert_eq!(creations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn store_instances_are_isolated_per_application() {
        let first = ApplicationContext::empty(crate::memory::test_memory_options());
        let second = ApplicationContext::empty(crate::memory::test_memory_options());

        first.update_store::<ValueStore, _>("first", |store| store.0 = 7);

        assert_eq!(first.read_store::<ValueStore, _>(|store| store.0), 7);
        assert_eq!(second.read_store::<ValueStore, _>(|store| store.0), 0);
    }

    #[test]
    fn store_updates_wake_the_owning_application() {
        let application = ApplicationContext::empty(crate::memory::test_memory_options());
        let wakes = Arc::new(AtomicUsize::new(0));
        application.stores().set_wake({
            let wakes = Arc::clone(&wakes);
            Arc::new(move || {
                wakes.fetch_add(1, Ordering::SeqCst);
            })
        });

        application.update_store::<ValueStore, _>("wake", |store| store.0 += 1);

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn application_drop_releases_its_store_instances() {
        let drops = Arc::new(AtomicUsize::new(0));
        {
            let application = context_with_counter(Arc::clone(&drops));
            application.read_store::<DroppedStore, _>(|_| ());
            assert_eq!(drops.load(Ordering::SeqCst), 0);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}
