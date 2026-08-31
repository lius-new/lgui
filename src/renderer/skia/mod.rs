mod backend;

pub use backend::probe_skia_support;
pub(crate) use backend::{
    cpu_cache_budget, gpu_cache_budget, paint_scene_damage, skia_text_system_handle,
    with_gpu_cache_usage, SkiaCache, SkiaCacheStats, SkiaSoftwareSurface, DEFAULT_CACHE_BUDGET,
};
