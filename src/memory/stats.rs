use super::{CacheDomain, DomainInstanceId, MemoryOptions, TrimReason};
#[cfg(any(
    feature = "images-win32",
    feature = "renderer-d2d",
    all(
        feature = "svg",
        any(feature = "backend-win32", feature = "tray-win32")
    )
))]
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

    pub(crate) fn add_assign(&mut self, other: Self) {
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

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainSnapshot {
    pub registration_id: u64,
    pub domain: CacheDomain,
    pub instance: DomainInstanceId,
    pub owner: String,
    pub usage: CacheUsage,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrimSnapshot {
    pub reason: TrimReason,
    pub requested_at_epoch: u64,
    pub released_bytes: usize,
    pub duration_micros: u64,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySnapshot {
    pub epoch: u64,
    pub options: MemoryOptions,
    pub usage: CacheUsage,
    pub transient_reserved_bytes: usize,
    pub large_tasks_in_flight: usize,
    pub pinned_overflow_bytes: usize,
    pub domains: Vec<DomainSnapshot>,
    pub last_trim: Option<TrimSnapshot>,
}

#[cfg(any(
    feature = "images-win32",
    feature = "renderer-d2d",
    all(
        feature = "svg",
        any(feature = "backend-win32", feature = "tray-win32")
    )
))]
#[derive(Clone, Default)]
pub(crate) struct CacheTelemetry {
    usage: Arc<Mutex<CacheUsage>>,
}

#[cfg(any(
    feature = "images-win32",
    feature = "renderer-d2d",
    all(
        feature = "svg",
        any(feature = "backend-win32", feature = "tray-win32")
    )
))]
impl CacheTelemetry {
    pub(crate) fn publish(&self, usage: CacheUsage) {
        *self.usage.lock().expect("cache telemetry poisoned") = usage;
    }

    #[cfg(any(
        feature = "images-win32",
        feature = "renderer-d2d",
        all(
            feature = "svg",
            feature = "backend-win32",
            any(
                feature = "renderer-gdi",
                feature = "renderer-d2d",
                feature = "renderer-skia"
            )
        )
    ))]
    pub(crate) fn snapshot(&self) -> CacheUsage {
        *self.usage.lock().expect("cache telemetry poisoned")
    }
}
