use std::sync::{Arc, Mutex};

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheUsage {
    pub live_bytes: usize,
    pub rebuildable_bytes: usize,
    pub cache_bytes: usize,
    pub cpu_bytes: usize,
    pub gpu_estimated_bytes: usize,
    pub pinned_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub rebuilds: u64,
    pub largest_entry_bytes: usize,
    pub in_flight: usize,
}

impl CacheUsage {
    pub const fn resident_bytes(self) -> usize {
        self.live_bytes
            .saturating_add(self.rebuildable_bytes)
            .saturating_add(self.cache_bytes)
    }

    pub const fn managed_bytes(self) -> usize {
        self.rebuildable_bytes.saturating_add(self.cache_bytes)
    }

    #[doc(hidden)]
    pub fn add_assign(&mut self, other: Self) {
        self.live_bytes = self.live_bytes.saturating_add(other.live_bytes);
        self.rebuildable_bytes = self
            .rebuildable_bytes
            .saturating_add(other.rebuildable_bytes);
        self.cache_bytes = self.cache_bytes.saturating_add(other.cache_bytes);
        self.cpu_bytes = self.cpu_bytes.saturating_add(other.cpu_bytes);
        self.gpu_estimated_bytes = self
            .gpu_estimated_bytes
            .saturating_add(other.gpu_estimated_bytes);
        self.pinned_bytes = self.pinned_bytes.saturating_add(other.pinned_bytes);
        self.entries = self.entries.saturating_add(other.entries);
        self.hits = self.hits.saturating_add(other.hits);
        self.misses = self.misses.saturating_add(other.misses);
        self.evictions = self.evictions.saturating_add(other.evictions);
        self.rebuilds = self.rebuilds.saturating_add(other.rebuilds);
        self.largest_entry_bytes = self.largest_entry_bytes.max(other.largest_entry_bytes);
        self.in_flight = self.in_flight.saturating_add(other.in_flight);
    }
}

#[derive(Clone, Default)]
#[doc(hidden)]
pub struct CacheTelemetry {
    usage: Arc<Mutex<CacheUsage>>,
}

impl CacheTelemetry {
    pub fn publish(&self, usage: CacheUsage) {
        *self.usage.lock().expect("cache telemetry poisoned") = usage;
    }

    pub fn snapshot(&self) -> CacheUsage {
        *self.usage.lock().expect("cache telemetry poisoned")
    }
}
