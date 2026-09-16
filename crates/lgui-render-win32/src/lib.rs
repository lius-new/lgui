#![cfg(target_os = "windows")]

#[cfg(feature = "gdi")]
#[path = "renderer/backbuffer.rs"]
mod backbuffer;
#[cfg(feature = "d2d")]
#[path = "renderer/d2d.rs"]
mod d2d;
#[cfg(any(feature = "advanced-rendering", feature = "d2d"))]
#[path = "renderer/enhanced/mod.rs"]
pub mod enhanced;
#[cfg(feature = "gdi")]
#[path = "renderer/gdi.rs"]
mod gdi;
#[cfg(feature = "diagnostics")]
#[path = "renderer/render_trace.rs"]
pub mod render_trace;

#[cfg(any(feature = "advanced-rendering", feature = "d2d"))]
mod environment;
#[cfg(feature = "gdi")]
mod gdi_factory;

#[cfg(all(feature = "gdi", feature = "multi-window"))]
pub use backbuffer::{rect_size, AlphaPolicy, LayeredBackbuffer};
#[cfg(feature = "d2d")]
pub use d2d::{probe_d2d_support, D2dRenderer, D2dRendererFactory};
#[cfg(feature = "gdi")]
pub use gdi::GdiRenderer;
#[cfg(feature = "gdi")]
pub use gdi_factory::GdiRendererFactory;

pub use lgui_platform_win32::{
    Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
};
