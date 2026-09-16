//! Internal renderer cache controls and shared rendering support.

mod cache;
#[cfg(feature = "raster-effects")]
pub(crate) mod shadow;

pub(crate) use cache::install_render_cache;
pub use cache::*;
