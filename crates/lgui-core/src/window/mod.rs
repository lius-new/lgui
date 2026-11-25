//! Platform-neutral window identity, configuration, handles, and commands.

use std::sync::Arc;

use crate::core::{Element, RenderCx};

mod command;
mod id;
mod manager;
mod options;

#[doc(hidden)]
pub use command::WindowCommand;
pub use id::WindowId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowFocusChanged {
    pub window_id: WindowId,
    pub focused: bool,
}

impl crate::events::Event for WindowFocusChanged {
    const NAME: &'static str = "window.focus_changed";
}

/// The pointer left the window's client area, so nothing in it is hovered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowPointerLeft {
    pub window_id: WindowId,
}

impl crate::events::Event for WindowPointerLeft {
    const NAME: &'static str = "window.pointer_left";
}
pub use manager::{WindowHandle, WindowManager};
pub use options::{
    ClosePolicy, WindowCloseHandler, WindowDragExclusion, WindowMode, WindowOptions, WindowPosition,
};

#[doc(hidden)]
pub type WindowView = Arc<
    dyn for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
>;

#[path = "window_test.rs"]
#[cfg(test)]
mod tests;
