//! Application-scoped memory policy, safety limits, and cache accounting.

mod governor;
mod lru;
mod options;
mod policy;
mod stats;

#[cfg(feature = "persistent-cache")]
mod persistent;

pub use governor::{
    DomainInstanceId, MemoryGovernor, MemoryReservation, MemoryTaskReservation,
};
#[doc(hidden)]
pub use lru::LruCache;
pub use options::{MemoryBudget, MemoryOptions};
pub use policy::{CachePriority, ImageCachePolicy, ResourceClass, RetentionClass};
#[doc(hidden)]
pub use stats::CacheTelemetry;
pub use stats::CacheUsage;

#[cfg(feature = "persistent-cache")]
pub use persistent::{
    CacheStoreError, FileCacheStore, PersistentCacheKey, PersistentCacheStats,
    PersistentCacheStore, PersistentEntry,
};

#[path = "memory_test.rs"]
#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub const fn test_memory_options() -> MemoryOptions {
    MemoryOptions::new(
        MemoryBudget::new(64 * 1024 * 1024),
        ImageCachePolicy::Session,
        true,
    )
}

#[cfg(all(feature = "persistent-cache", any(test, feature = "test-support")))]
#[doc(hidden)]
pub fn test_memory_governor_with_store(
    options: MemoryOptions,
    store: std::sync::Arc<dyn PersistentCacheStore>,
) -> MemoryGovernor {
    MemoryGovernor::with_store(options, Some(store))
}
