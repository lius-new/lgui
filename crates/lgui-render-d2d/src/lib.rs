//! Windows Direct2D renderer backend for LGUI.

#![cfg(target_os = "windows")]

#[doc(hidden)]
pub mod backend;
mod environment;
mod renderer;

pub(crate) use lgui_platform_win32::render_support::{blur, image, trace as render_trace};

pub use lgui_platform_win32::{
    Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
};
pub use renderer::{probe_d2d_support, D2dRenderer, D2dRendererFactory};
