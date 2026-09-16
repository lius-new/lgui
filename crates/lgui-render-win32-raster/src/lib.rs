//! Shared Win32 raster and cache support for concrete LGUI renderers.

#![cfg(target_os = "windows")]

#[cfg(any(feature = "gdi", feature = "d2d"))]
#[doc(hidden)]
pub mod backend;
#[cfg(any(feature = "gdi", feature = "d2d"))]
pub mod blur;
#[cfg(any(feature = "gdi", feature = "d2d"))]
pub mod image;
#[cfg(feature = "gdi")]
mod render_cache;
#[cfg(any(feature = "gdi", feature = "d2d"))]
pub mod render_trace;
#[cfg(any(feature = "gdi", feature = "d2d"))]
pub mod static_layer;
