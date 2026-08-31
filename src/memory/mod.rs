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
pub use options::{MemoryBudget, MemoryOptions, MemoryProfile};
pub use policy::{
    CacheDomain, CacheKey, CachePriority, CacheScope, MemoryEvent, ResourceClass, RetentionClass,
    TrimReason,
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

#[cfg(test)]
mod tests;
