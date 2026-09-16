//! Application construction, context, window management, and backend selection.

mod backend;
mod builder;
mod context;
mod error;
mod handle;
mod renderer_selection;
mod scope;
#[cfg(all(
    feature = "tray-win32",
    any(feature = "backend-win32", feature = "backend-winit")
))]
mod tray;
mod view;

pub use crate::renderer::RenderErrorStage;

#[cfg(feature = "tray")]
pub use crate::services::{TrayAction, TrayOptions};
pub use crate::window::{
    ClosePolicy, WindowCloseHandler, WindowDragExclusion, WindowFocusChanged, WindowHandle,
    WindowId, WindowManager, WindowMode, WindowOptions, WindowPosition,
};
pub use backend::ApplicationBackend;
#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
pub use backend::{DesktopApplication, DesktopApplicationError};
pub use builder::{Application, MemoryOptionsConfigured, MemoryOptionsMissing};
pub use context::ApplicationContext;
pub use error::RenderError;
pub(crate) use error::RenderErrorRegistration;
pub use handle::{ApplicationHandle, ApplicationTask};
#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
pub use renderer_selection::RendererKind;
pub use renderer_selection::{GraphicsPreference, RendererProbeError};
pub(crate) use scope::{current_application, ApplicationScopeFuture};
#[cfg(all(
    feature = "tray-win32",
    any(feature = "backend-win32", feature = "backend-winit")
))]
pub(crate) use tray::{dispatch_tray_action, TrayRegistration};
pub(crate) use view::application_root_view;
pub use view::AppView;

#[path = "application_test.rs"]
#[cfg(test)]
mod tests;
