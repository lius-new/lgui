#[cfg(any(feature = "renderer-gdi", feature = "renderer-d2d"))]
mod application;
mod backbuffer;
mod clipboard;
#[cfg(feature = "renderer-d2d")]
mod d2d;
mod dispatcher;
mod dpi;
#[cfg(feature = "renderer-gdi")]
mod gdi;
mod window_host;

#[cfg(any(feature = "renderer-gdi", feature = "renderer-d2d"))]
pub use application::{GdiRendererFactory, Win32Application, Win32Renderer, Win32RendererFactory};
pub use backbuffer::{rect_size, AlphaPolicy, LayeredBackbuffer};
pub use clipboard::{read_clipboard_text, write_clipboard_text, Win32Clipboard};
#[cfg(feature = "renderer-d2d")]
pub use d2d::{D2dRenderer, D2dRendererFactory};
pub use dispatcher::{Win32DispatchResult, Win32Dispatcher, WM_LGUI_DISPATCH};
pub use dpi::{set_scale_preference, work_area_for_point, work_area_for_window, DpiContext};
#[cfg(feature = "renderer-gdi")]
pub use gdi::GdiRenderer;
pub use window_host::{
    DragExclusionRect, LayeredWindowConfig, LayeredWindowErrorReporter, LayeredWindowEventDispatch,
    LayeredWindowEventDispatcher, LayeredWindowHost, LayeredWindowPosition,
    LayeredWindowSessionConfigurator, LayeredWindowTreeBuilder,
};
