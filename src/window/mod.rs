//! Platform-neutral window identity, configuration, handles, and commands.

use std::sync::Arc;

use crate::core::{Element, RenderCx};

mod command;
mod id;
mod manager;
mod options;

pub(crate) use command::WindowCommand;
pub use id::WindowId;
pub use manager::{WindowHandle, WindowManager};
pub use options::{
    ClosePolicy, WindowCloseHandler, WindowDragExclusion, WindowMode, WindowOptions, WindowPosition,
};

pub(crate) type WindowView = Arc<
    dyn for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
>;

#[cfg(test)]
mod tests;
