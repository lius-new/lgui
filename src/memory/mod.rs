//! Application-scoped resource accounting, cache policy, pressure, and persistence.

mod governor;
#[cfg(any(
    feature = "images-win32",
    feature = "renderer-d2d",
    all(
        feature = "svg",
        any(feature = "backend-win32", feature = "tray-win32")
    )
))]
mod lru;
mod options;
mod policy;
mod registry;
mod stats;

#[cfg(feature = "persistent-cache")]
mod persistent;

pub use governor::{MemoryGovernor, MemoryReservation, MemoryTaskReservation};
#[cfg(any(
    feature = "images-win32",
    feature = "renderer-d2d",
    all(
        feature = "svg",
        any(feature = "backend-win32", feature = "tray-win32")
    )
))]
pub(crate) use lru::LruCache;
pub use options::{MemoryBudget, MemoryDomainBudgets, MemoryEventPolicy, MemoryOptions};
pub use policy::{
    CacheDomain, CacheKey, CachePriority, CacheScope, ImageCachePolicy, MemoryAction, MemoryEvent,
    ResourceClass, RetentionClass, TrimReason,
};
pub use registry::{
    CacheAdapter, CacheRegistration, DomainInstanceId, DomainRegistration, TrimRequest, TrimResult,
};
#[cfg(any(
    feature = "images-win32",
    feature = "renderer-d2d",
    all(
        feature = "svg",
        any(feature = "backend-win32", feature = "tray-win32")
    )
))]
pub(crate) use stats::CacheTelemetry;
pub use stats::{CacheUsage, DomainSnapshot, MemorySnapshot, TrimSnapshot};

#[cfg(feature = "persistent-cache")]
pub use persistent::{
    CacheStoreError, FileCacheStore, PersistentCacheKey, PersistentCacheStats,
    PersistentCacheStore, PersistentEntry,
};

#[path = "memory_test.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) const fn test_memory_options() -> MemoryOptions {
    const MIB: usize = 1024 * 1024;
    MemoryOptions::new(
        "lgui-tests",
        MemoryBudget::new(
            64 * MIB,
            96 * MIB,
            64 * MIB,
            64 * MIB as u64,
            8 * MIB,
            16 * MIB,
            4,
        ),
        MemoryDomainBudgets {
            encoded_image_bytes: 16 * MIB,
            decoded_image_bytes: 16 * MIB,
            svg_bytes: 4 * MIB,
            blur_bytes: 4 * MIB,
            text_bytes: 8 * MIB,
            static_layer_bytes: 8 * MIB,
            scroll_raster_bytes: 4 * MIB,
            gdi_bytes: 16 * MIB,
            d2d_bytes: 16 * MIB,
            skia_bytes: 16 * MIB,
            component_output_bytes: 4 * MIB,
            host_scene_bytes: 4 * MIB,
            diagnostics_bytes: 2 * MIB,
        },
        MemoryEventPolicy::ignore_all(),
        ImageCachePolicy::Session,
        true,
    )
}
