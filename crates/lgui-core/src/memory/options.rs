use super::ImageCachePolicy;

const DEFAULT_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Application memory budget.
///
/// A single `cache_bytes` ceiling is applied to every framework-owned cache
/// (renderer raster/text cache, decoded-image cache, scroll-raster command
/// cache, and so on). Each cache independently evicts its own entries to stay
/// within this ceiling, so total resident memory is bounded by the number of
/// live caches times `cache_bytes`.
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryBudget {
    pub cache_bytes: usize,
    /// Persistent store quota; only meaningful with the `persistent-cache`
    /// feature.
    pub persistent_bytes: u64,
}

impl MemoryBudget {
    pub const fn new(cache_bytes: usize) -> Self {
        Self {
            cache_bytes,
            persistent_bytes: 0,
        }
    }

    pub const fn with_persistent(cache_bytes: usize, persistent_bytes: u64) -> Self {
        Self {
            cache_bytes,
            persistent_bytes,
        }
    }
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_BYTES)
    }
}

/// Application-scoped memory policy.
///
/// The budget is the single user-facing knob; image cache policy and
/// persistent-cache toggle are orthogonal behavior switches. When no policy is
/// provided, [`MemoryOptions::default`] applies a 64 MiB per-cache budget with
/// `WhileVisible` image caching.
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryOptions {
    pub budget: MemoryBudget,
    pub default_image_cache_policy: ImageCachePolicy,
    pub persistent_cache_enabled: bool,
}

impl MemoryOptions {
    pub const fn new(
        budget: MemoryBudget,
        default_image_cache_policy: ImageCachePolicy,
        persistent_cache_enabled: bool,
    ) -> Self {
        Self {
            budget,
            default_image_cache_policy,
            persistent_cache_enabled,
        }
    }

    pub const fn cache_budget(cache_bytes: usize) -> Self {
        Self::new(
            MemoryBudget::new(cache_bytes),
            ImageCachePolicy::WhileVisible,
            false,
        )
    }

    pub const fn unbounded(
        default_image_cache_policy: ImageCachePolicy,
        persistent_cache_enabled: bool,
    ) -> Self {
        Self::new(
            MemoryBudget {
                cache_bytes: usize::MAX,
                persistent_bytes: u64::MAX,
            },
            default_image_cache_policy,
            persistent_cache_enabled,
        )
    }

    pub const fn persistent_cache(mut self, enabled: bool) -> Self {
        self.persistent_cache_enabled = enabled;
        self
    }

    pub const fn persistent_budget(mut self, bytes: u64) -> Self {
        self.budget.persistent_bytes = bytes;
        self
    }

    pub fn validate(self) -> Result<(), &'static str> {
        if self.default_image_cache_policy == ImageCachePolicy::ApplicationDefault {
            return Err("application default image policy must be concrete");
        }
        Ok(())
    }
}

impl Default for MemoryOptions {
    fn default() -> Self {
        Self::cache_budget(DEFAULT_CACHE_BYTES)
    }
}
