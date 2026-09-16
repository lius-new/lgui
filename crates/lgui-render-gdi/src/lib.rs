//! Windows GDI renderer backend for LGUI.

#![cfg(target_os = "windows")]

mod backbuffer;
#[cfg(feature = "advanced-rendering")]
#[doc(hidden)]
pub mod backend;
#[cfg(feature = "advanced-rendering")]
mod environment;
mod factory;
mod renderer;

#[cfg(feature = "advanced-rendering")]
pub(crate) use lgui_render_win32_raster::{blur, image, render_trace, static_layer};

#[cfg(all(feature = "multi-window", target_os = "windows"))]
pub use backbuffer::{rect_size, AlphaPolicy, LayeredBackbuffer};
pub use factory::GdiRendererFactory;
pub use renderer::GdiRenderer;

pub use lgui_platform_win32::{
    Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
};
