//! Backend-neutral rendering contracts, cache controls, and renderer implementations.

mod cache;
mod contract;

pub(crate) use cache::install_render_cache;
pub use cache::*;
pub use contract::*;

#[cfg(feature = "renderer-skia")]
#[allow(unsafe_code)]
pub(crate) mod skia;
