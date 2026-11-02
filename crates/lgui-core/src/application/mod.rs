//! Application construction, context, window management, and backend selection.

mod backend;
mod builder;
mod context;
mod error;
mod handle;
mod scope;
mod view;

pub use crate::window::{
    ClosePolicy, WindowCloseHandler, WindowDragExclusion, WindowFocusChanged, WindowHandle,
    WindowId, WindowManager, WindowMode, WindowOptions, WindowPosition,
};
pub use backend::ApplicationBackend;
pub use builder::Application;
pub use context::ApplicationContext;
pub(crate) use error::RenderErrorRegistration;
pub use error::{RenderError, RenderErrorStage};
pub use handle::{ApplicationHandle, ApplicationTask};
pub(crate) use scope::{ApplicationScopeFuture, current_application};
pub use view::AppView;
pub(crate) use view::application_root_view;

#[path = "application_test.rs"]
#[cfg(test)]
mod tests;
