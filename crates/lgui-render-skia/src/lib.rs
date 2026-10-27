#![deny(unsafe_code)]

#[allow(unsafe_code)]
mod backend;

pub use backend::probe_skia_support;
#[cfg(any(
    feature = "renderer-skia-gl",
    feature = "renderer-skia-vulkan",
    feature = "renderer-skia-metal"
))]
pub use backend::{
    SkiaCache, cpu_cache_budget, gpu_cache_budget, paint_scene_damage, with_gpu_cache_usage,
};
pub use backend::{
    SkiaCacheStats, SkiaSoftwareSurface, TextRasterization, set_opaque_software_windows,
    set_software_text_background, skia_text_system_handle,
};
