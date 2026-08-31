use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
};

use crate::{
    command::Command,
    core::{Size, UiRect},
    platform::dpi::ScalePreference,
    renderer::RenderErrorStage,
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
        .command::<BuilderCommand>(|_, value| async move { Ok(value + 1) })
        .run(|cx| crate::core::group(cx.viewport()))
        .expect("invoking backend should run");

    assert_eq!(
        *result.lock().expect("command result lock poisoned"),
        Some(42)
    );
}

#[test]
fn render_errors_are_delivered_with_structured_context() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&errors);

    Application::with_backend(ReportingBackend)
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
fn native_titlebar_is_enabled_by_default_and_can_be_disabled() {
    assert!(WindowOptions::default().native_titlebar);
    assert!(WindowOptions::new("default").native_titlebar);
    assert!(
        !WindowOptions::new("custom")
            .native_titlebar(false)
            .native_titlebar
    );
}

#[test]
fn windows_are_visible_by_default_and_can_start_hidden() {
    assert!(WindowOptions::default().visible);
    assert!(!WindowOptions::new("background").visible(false).visible);
}

#[test]
fn window_size_constraints_are_optional_and_configurable() {
    let defaults = WindowOptions::new("default");
    assert_eq!(defaults.minimum_size, None);
    assert_eq!(defaults.maximum_size, None);

    let constrained = defaults
        .minimum_size(Size::new(640.0, 360.0))
        .maximum_size(Size::new(1920.0, 1080.0));
    assert_eq!(constrained.minimum_size, Some(Size::new(640.0, 360.0)));
    assert_eq!(constrained.maximum_size, Some(Size::new(1920.0, 1080.0)));
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

#[test]
fn auxiliary_window_options_are_declared_without_platform_handles() {
    let options = WindowOptions::new("friends")
        .owner("main")
        .title("Friends")
        .size(Size::new(292.0, 640.0))
        .minimum_size(Size::new(292.0, 360.0))
        .position(WindowPosition::AdjacentToOwner { gap: 1 })
        .transparent(true)
        .corner_radius(8)
        .background_memory_optimization(true);

    assert_eq!(options.id.as_str(), "friends");
    assert_eq!(options.owner.as_ref().map(WindowId::as_str), Some("main"));
    assert_eq!(options.position, WindowPosition::AdjacentToOwner { gap: 1 });
    assert!(options.transparent);
    assert!(options.background_memory_optimization);
}

#[test]
fn window_manager_marshals_the_complete_command_model() {
    let manager = WindowManager::new();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&commands);
    manager.install(move |command| {
        let label = match command {
            WindowCommand::Show { options, .. } => format!("show:{}", options.id.as_str()),
            WindowCommand::Hide(id) => format!("hide:{}", id.as_str()),
            WindowCommand::Toggle { options, .. } => {
                format!("toggle:{}", options.id.as_str())
            }
            WindowCommand::Close(id) => format!("close:{}", id.as_str()),
            WindowCommand::RequestClose(id) => format!("request-close:{}", id.as_str()),
            WindowCommand::Minimize(id) => format!("minimize:{}", id.as_str()),
            WindowCommand::SetScalePreference(ScalePreference::Auto) => "scale:auto".into(),
            WindowCommand::SetScalePreference(ScalePreference::Multiplier(value)) => {
                format!("scale:{value}")
            }
            WindowCommand::SetMode { id, mode } => {
                format!("mode:{}:{mode:?}", id.as_str())
            }
            WindowCommand::Input { id, .. } => format!("input:{}", id.as_str()),
            WindowCommand::Exit => "exit".into(),
        };
        recorded.lock().expect("command log poisoned").push(label);
    });

    manager.show(WindowOptions::new("friends"), |_| {
        crate::core::content_text("friends")
    });
    manager.hide("friends");
    manager.toggle(WindowOptions::new("friends"), |_| {
        crate::core::content_text("friends")
    });
    manager.set_scale_preference(ScalePreference::Multiplier(0.9));
    manager.set_mode("friends", WindowMode::Fullscreen);
    manager.request_close("friends");
    manager.close("friends");
    manager.exit();

    assert_eq!(
        *commands.lock().expect("command log poisoned"),
        [
            "show:friends",
            "hide:friends",
            "toggle:friends",
            "scale:0.9",
            "mode:friends:Fullscreen",
            "request-close:friends",
            "close:friends",
            "exit",
        ]
    );
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
        let first = ApplicationContext::empty();
        let second = ApplicationContext::empty();

        first.update_store::<ValueStore, _>("first", |store| store.0 = 7);

        assert_eq!(first.read_store::<ValueStore, _>(|store| store.0), 7);
        assert_eq!(second.read_store::<ValueStore, _>(|store| store.0), 0);
    }

    #[test]
    fn store_updates_wake_the_owning_application() {
        let application = ApplicationContext::empty();
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

#[cfg(all(
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    target_os = "windows"
))]
#[test]
fn gdi_and_d2d_use_the_same_application_builder_type() {
    #[cfg(not(all(feature = "backend-winit", feature = "renderer-skia")))]
    fn assert_type(_: Application<crate::platform::win32::Win32Application>) {}
    #[cfg(all(feature = "backend-winit", feature = "renderer-skia"))]
    fn assert_type(_: Application<DesktopApplication>) {}

    assert_type(Application::new().renderer(RendererKind::Gdi));
    assert_type(Application::new().renderer(RendererKind::D2d));
}
