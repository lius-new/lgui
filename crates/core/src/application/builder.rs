use std::sync::Arc;

use crate::{
    command::{Command, CommandHandler, CommandRegistry},
    core::{Element, RenderCx, UiExecutor, UiTaskSpawner},
    events::EventBus,
    memory::MemoryOptions,
    resources::Resources,
};

#[cfg(feature = "persistent-cache")]
use crate::memory::PersistentCacheStore;

use super::{
    application_root_view, AppView, ApplicationBackend, ApplicationContext, RenderError,
    RenderErrorRegistration, WindowOptions,
};

#[cfg(all(
    feature = "tray-win32",
    any(feature = "backend-win32", feature = "backend-winit")
))]
use super::{TrayOptions, TrayRegistration};

pub struct MemoryOptionsMissing;

pub struct MemoryOptionsConfigured(MemoryOptions);

pub struct Application<B, M = MemoryOptionsMissing> {
    backend: B,
    window: WindowOptions,
    resources: Resources,
    executor: Option<UiTaskSpawner>,
    commands: CommandRegistry,
    events: EventBus,
    memory_options: M,
    #[cfg(feature = "persistent-cache")]
    persistent_cache: Option<Arc<dyn PersistentCacheStore>>,
}

impl<B> Application<B, MemoryOptionsMissing> {
    pub fn with_backend(backend: B) -> Self {
        Self {
            backend,
            window: WindowOptions::default(),
            resources: Resources::new(),
            executor: None,
            commands: CommandRegistry::default(),
            events: EventBus::default(),
            memory_options: MemoryOptionsMissing,
            #[cfg(feature = "persistent-cache")]
            persistent_cache: None,
        }
    }

    pub fn memory_options(self, options: MemoryOptions) -> Application<B, MemoryOptionsConfigured> {
        options
            .validate()
            .expect("invalid application memory policy");
        Application {
            backend: self.backend,
            window: self.window,
            resources: self.resources,
            executor: self.executor,
            commands: self.commands,
            events: self.events,
            memory_options: MemoryOptionsConfigured(options),
            #[cfg(feature = "persistent-cache")]
            persistent_cache: self.persistent_cache,
        }
    }
}

impl<B, M> Application<B, M> {
    pub fn window_options(mut self, options: WindowOptions) -> Self {
        self.window = options;
        self
    }

    pub fn provide<T>(self, value: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.resources.provide(value);
        self
    }

    pub fn executor(mut self, executor: impl UiExecutor) -> Self {
        self.executor = Some(Arc::new(executor));
        self
    }

    #[cfg(feature = "persistent-cache")]
    pub fn persistent_cache(mut self, store: impl PersistentCacheStore) -> Self {
        self.persistent_cache = Some(Arc::new(store));
        self
    }

    pub fn command<C>(self, handler: impl CommandHandler<C>) -> Self
    where
        C: Command,
    {
        assert!(
            self.commands.register::<C>(handler),
            "command `{}` is already registered",
            C::NAME
        );
        self
    }

    pub fn on_render_error(self, handler: impl Fn(&RenderError) + Send + Sync + 'static) -> Self {
        self.resources
            .provide(RenderErrorRegistration::new(handler));
        self
    }

    #[cfg(feature = "diagnostics")]
    pub fn diagnostics_sink(
        self,
        sink: impl crate::diagnostics::DiagnosticsSink + 'static,
    ) -> Self {
        self.resources
            .provide(crate::diagnostics::DiagnosticsRegistration::new(sink));
        self
    }

    pub fn font_families(self, families: &'static [&'static str]) -> Self {
        self.resources.provide(crate::text::FontFamilies(families));
        self
    }

    #[cfg(feature = "renderer-skia")]
    pub fn font_assets(self, assets: Vec<crate::text::FontAsset>) -> Self {
        self.resources
            .provide(crate::text::FontAssets(std::sync::Arc::new(assets)));
        self
    }

    #[cfg(feature = "svg")]
    pub fn svg_icons(self, registry: crate::icons::SvgIconRegistry) -> Self {
        self.resources
            .provide(crate::icons::IconRegistration(std::sync::Arc::new(
                registry,
            )));
        self
    }

    #[cfg(all(
        feature = "tray-win32",
        any(feature = "backend-win32", feature = "backend-winit")
    ))]
    pub fn tray(
        self,
        options: TrayOptions,
        handler: impl Fn(&ApplicationContext, &str) + Send + Sync + 'static,
    ) -> Self {
        self.resources.provide(TrayRegistration {
            options,
            handler: Arc::new(handler),
        });
        self
    }

    #[cfg(feature = "notifications")]
    pub fn notification_service(self, service: crate::services::NotificationHandle) -> Self {
        self.resources.provide(service);
        self
    }
}

impl<B> Application<B, MemoryOptionsConfigured>
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
        #[cfg(feature = "clipboard")]
        if self
            .resources
            .get::<crate::services::ClipboardHandle>()
            .is_none()
        {
            self.resources
                .provide::<crate::services::ClipboardHandle>(crate::clipboard::system_clipboard());
        }
        #[cfg(feature = "open-url")]
        if self
            .resources
            .get::<crate::desktop::OpenUrlHandle>()
            .is_none()
        {
            self.resources.provide(crate::desktop::system_url_opener());
        }
        #[cfg(feature = "dialogs")]
        if self
            .resources
            .get::<crate::dialogs::FileDialogHandle>()
            .is_none()
        {
            self.resources
                .provide(crate::dialogs::system_file_dialogs());
        }
        let context = ApplicationContext::new_with_memory(
            self.resources,
            self.executor,
            self.commands,
            self.events,
            self.memory_options.0,
            #[cfg(feature = "persistent-cache")]
            self.persistent_cache,
        );
        let backend_context = context.clone();
        let view: AppView = Arc::new(view);
        let root = application_root_view(context, view);
        self.backend.run(self.window, root, backend_context)
    }
}
