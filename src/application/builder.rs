use std::sync::Arc;

use crate::{
    command::{Command, CommandHandler, CommandRegistry},
    core::{Element, RenderCx, UiExecutor, UiTaskSpawner},
    events::EventBus,
    resources::Resources,
};

use super::{
    application_root_view, AppView, ApplicationBackend, ApplicationContext, RenderError,
    RenderErrorRegistration, WindowOptions,
};

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
use super::DesktopApplication;
#[cfg(feature = "renderer-skia")]
use super::GraphicsPreference;
#[cfg(feature = "notifications")]
use super::NotificationRegistration;
#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-skia", feature = "backend-winit")
))]
use super::RendererKind;
#[cfg(feature = "tray")]
use super::{TrayOptions, TrayRegistration};

pub struct Application<B> {
    backend: B,
    window: WindowOptions,
    resources: Resources,
    executor: Option<UiTaskSpawner>,
    commands: CommandRegistry,
    events: EventBus,
}

impl<B> Application<B> {
    pub fn with_backend(backend: B) -> Self {
        Self {
            backend,
            window: WindowOptions::default(),
            resources: Resources::new(),
            executor: None,
            commands: CommandRegistry::default(),
            events: EventBus::default(),
        }
    }

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

    #[cfg(feature = "tray")]
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
    pub fn notifications(self, identity: impl Into<String>) -> Self {
        self.resources.provide(NotificationRegistration {
            identity: identity.into(),
        });
        self
    }
}

#[cfg(all(
    feature = "renderer-gdi",
    target_os = "windows",
    not(all(feature = "backend-winit", feature = "renderer-skia"))
))]
impl Application<crate::platform::win32::Win32Application> {
    pub fn new() -> Self {
        Self::with_backend(crate::platform::win32::Win32Application::default())
            .provide(RendererKind::Gdi)
    }

    pub fn renderer(mut self, renderer: RendererKind) -> Self {
        self.resources.provide(renderer);
        self.backend = match renderer {
            RendererKind::Gdi => crate::platform::win32::Win32Application::with_renderer(
                crate::platform::win32::GdiRendererFactory,
            ),
            #[cfg(feature = "renderer-d2d")]
            RendererKind::D2d => crate::platform::win32::Win32Application::with_renderer(
                crate::platform::win32::D2dRendererFactory,
            ),
            #[cfg(feature = "renderer-skia")]
            RendererKind::Skia(_) => {
                panic!("the Skia desktop renderer requires the backend-winit feature")
            }
        };
        self
    }
}

#[cfg(all(
    feature = "renderer-gdi",
    target_os = "windows",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl Application<DesktopApplication> {
    pub fn new() -> Self {
        Self::with_backend(DesktopApplication::Win32(
            crate::platform::win32::Win32Application::default(),
        ))
        .provide(RendererKind::Gdi)
    }

    pub fn renderer(mut self, renderer: RendererKind) -> Self {
        self.resources.provide(renderer);
        self.backend = match renderer {
            RendererKind::Gdi => {
                DesktopApplication::Win32(crate::platform::win32::Win32Application::with_renderer(
                    crate::platform::win32::GdiRendererFactory,
                ))
            }
            #[cfg(feature = "renderer-d2d")]
            RendererKind::D2d => {
                DesktopApplication::Win32(crate::platform::win32::Win32Application::with_renderer(
                    crate::platform::win32::D2dRendererFactory,
                ))
            }
            RendererKind::Skia(preference) => {
                DesktopApplication::Winit(crate::platform::WinitApplication::new(preference))
            }
        };
        self
    }
}

#[cfg(all(feature = "renderer-skia", feature = "backend-winit"))]
impl Application<crate::platform::WinitApplication> {
    pub fn new_skia(preference: GraphicsPreference) -> Self {
        Self::with_backend(crate::platform::WinitApplication::new(preference))
            .provide(RendererKind::Skia(preference))
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
        #[cfg(feature = "clipboard")]
        if self
            .resources
            .get::<crate::platform::ClipboardHandle>()
            .is_none()
        {
            self.resources
                .provide::<crate::platform::ClipboardHandle>(crate::clipboard::system_clipboard());
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
        let context =
            ApplicationContext::new(self.resources, self.executor, self.commands, self.events);
        let backend_context = context.clone();
        let view: AppView = Arc::new(view);
        let root = application_root_view(context, view);
        self.backend.run(self.window, root, backend_context)
    }
}
