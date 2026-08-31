//! Application construction, context, window management, and backend selection.

mod backend;
mod builder;
mod context;
mod error;
mod handle;
#[cfg(feature = "notifications")]
mod notification;
mod renderer_selection;
#[cfg(feature = "tray")]
mod tray;
mod view;
mod window;

pub use crate::renderer::RenderErrorStage;

pub use backend::ApplicationBackend;
#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
pub use backend::{DesktopApplication, DesktopApplicationError};
pub use builder::Application;
pub use context::ApplicationContext;
pub use error::RenderError;
pub(crate) use error::RenderErrorRegistration;
pub use handle::{ApplicationHandle, ApplicationTask};
#[cfg(feature = "notifications")]
pub(crate) use notification::NotificationRegistration;
#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
pub use renderer_selection::RendererKind;
pub use renderer_selection::{GraphicsPreference, RendererProbeError};
#[cfg(feature = "tray")]
pub(crate) use tray::TrayRegistration;
#[cfg(feature = "tray")]
pub use tray::{TrayAction, TrayOptions};
pub(crate) use view::application_root_view;
pub use view::AppView;
pub(crate) use window::WindowCommand;
pub use window::{
    ClosePolicy, WindowCloseHandler, WindowDragExclusion, WindowHandle, WindowId, WindowManager,
    WindowMode, WindowOptions, WindowPosition,
};

#[cfg(test)]
mod tests;
