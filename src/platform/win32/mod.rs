#[cfg(feature = "renderer-gdi")]
mod application;
mod backbuffer;
mod clipboard;
mod dpi;
#[cfg(feature = "renderer-gdi")]
mod gdi;
mod window_host;

#[cfg(feature = "renderer-gdi")]
pub use application::Win32Application;
pub use backbuffer::{rect_size, AlphaPolicy, LayeredBackbuffer};
pub use clipboard::{read_clipboard_text, write_clipboard_text, Win32Clipboard};
pub use dpi::{set_scale_preference, work_area_for_point, work_area_for_window, DpiContext};
#[cfg(feature = "renderer-gdi")]
pub use gdi::GdiRenderer;
pub use window_host::{
    DragExclusionRect, LayeredWindowConfig, LayeredWindowErrorReporter, LayeredWindowEventDispatch,
    LayeredWindowEventDispatcher, LayeredWindowHost, LayeredWindowPosition,
    LayeredWindowSessionConfigurator, LayeredWindowTreeBuilder,
};
