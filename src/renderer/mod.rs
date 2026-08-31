//! Backend-neutral rendering contracts, cache controls, and renderer implementations.

mod cache;
mod contract;

#[cfg(any(test, all(target_os = "windows", feature = "advanced-rendering")))]
pub(crate) use cache::install_render_cache;
pub use cache::*;
pub use contract::*;

#[cfg(feature = "renderer-skia")]
#[allow(unsafe_code)]
pub(crate) mod skia;
