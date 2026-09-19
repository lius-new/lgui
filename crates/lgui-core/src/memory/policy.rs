use std::time::Duration;

/// How long a cached resource is retained before it may be evicted.
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetentionClass {
    Frame,
    WhileVisible,
    Scene,
    #[default]
    Session,
    Persistent,
}

/// Eviction priority for a cached resource.
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum CachePriority {
    Low,
    #[default]
    Normal,
    High,
}

/// Per-image caching policy.
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageCachePolicy {
    #[default]
    ApplicationDefault,
    NoStore,
    WhileVisible,
    Scene,
    Session,
    Persistent {
        max_age: Duration,
        revalidate: bool,
    },
}

/// Internal accounting class used by the shared LRU cache.
#[doc(hidden)]
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum ResourceClass {
    Live,
    Rebuildable,
    #[default]
    Cache,
    Transient,
}
