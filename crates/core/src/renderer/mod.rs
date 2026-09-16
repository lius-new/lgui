//! Internal renderer cache controls and shared rendering support.

mod cache;
#[cfg(any(
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    feature = "renderer-skia"
))]
pub(crate) mod shadow;

#[cfg(any(test, all(target_os = "windows", feature = "advanced-rendering")))]
pub(crate) use cache::install_render_cache;
pub use cache::*;
