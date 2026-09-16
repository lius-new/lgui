use super::{AppView, ApplicationContext, WindowOptions};

pub trait ApplicationBackend: Sized {
    type Error;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error>;
}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
pub enum DesktopApplication {
    Win32(crate::platform::win32::Win32Application),
    Winit(crate::platform::WinitApplication),
}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
#[derive(Debug)]
pub struct DesktopApplicationError(String);

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl std::fmt::Display for DesktopApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl std::error::Error for DesktopApplicationError {}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl ApplicationBackend for DesktopApplication {
    type Error = DesktopApplicationError;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        match self {
            Self::Win32(backend) => backend
                .run(options, view, context)
                .map_err(|error| DesktopApplicationError(error.to_string())),
            Self::Winit(backend) => backend
                .run(options, view, context)
                .map_err(|error| DesktopApplicationError(error.to_string())),
        }
    }
}
