use lgui_core::{
    application::{AppView, ApplicationBackend, ApplicationContext},
    window::WindowOptions,
};

pub struct Win32Application(lgui_platform_win32::Win32Application);

impl Win32Application {
    pub fn with_renderer(factory: impl lgui_platform_win32::Win32RendererFactory) -> Self {
        Self(lgui_platform_win32::Win32Application::with_renderer(
            factory,
        ))
    }

    pub fn into_platform(self) -> lgui_platform_win32::Win32Application {
        self.0
    }
}

#[cfg(feature = "renderer-gdi")]
impl Default for Win32Application {
    fn default() -> Self {
        Self::with_renderer(lgui_render_win32::GdiRendererFactory)
    }
}

impl ApplicationBackend for Win32Application {
    type Error = <lgui_platform_win32::Win32Application as ApplicationBackend>::Error;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        self.0.run(options, view, context)
    }
}
