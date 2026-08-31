use std::{collections::HashMap, hash::Hash};

use super::{CacheTelemetry, CacheUsage, ResourceClass};

pub(crate) struct LruCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    bytes: usize,
    budget_bytes: usize,
    tick: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    class: ResourceClass,
    telemetry: CacheTelemetry,
}

struct Entry<V> {
    value: V,
    bytes: usize,
    last_used: u64,
}

impl<K, V> LruCache<K, V>
where
    K: Clone + Eq + Hash,
{
    pub(crate) fn new(
        budget_bytes: usize,
        class: ResourceClass,
        telemetry: CacheTelemetry,
    ) -> Self {
        let cache = Self {
            entries: HashMap::new(),
            bytes: 0,
            budget_bytes: budget_bytes.max(1),
            tick: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            class,
            telemetry,
        };
        cache.publish();
        cache
    }

    pub(crate) fn contains_touch(&mut self, key: &K) -> bool {
        self.tick = self.tick.wrapping_add(1);
        let present = if let Some(entry) = self.entries.get_mut(key) {
            entry.last_used = self.tick;
            self.hits = self.hits.saturating_add(1);
            true
        } else {
            self.misses = self.misses.saturating_add(1);
            false
        };
        self.publish();
        present
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|entry| &entry.value)
    }

    pub(crate) fn insert(&mut self, key: K, value: V, bytes: usize) -> bool {
        if bytes > self.budget_bytes {
            self.publish();
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        self.tick = self.tick.wrapping_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                last_used: self.tick,
            },
        );
        self.evict_to(self.budget_bytes);
        true
    }

    #[cfg(any(feature = "images-win32", feature = "renderer-d2d"))]
    pub(crate) fn can_store(&self, bytes: usize) -> bool {
        bytes <= self.budget_bytes
    }

    pub(crate) fn clear(&mut self) {
        self.evictions = self.evictions.saturating_add(self.entries.len() as u64);
        self.entries.clear();
        self.bytes = 0;
        self.publish();
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
    pub(crate) fn trim_to(&mut self, target_bytes: usize) -> usize {
        let before = self.bytes;
        self.evict_to(target_bytes);
        before.saturating_sub(self.bytes)
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
    pub(crate) fn set_budget(&mut self, budget_bytes: usize) {
        self.budget_bytes = budget_bytes.max(1);
        self.evict_to(self.budget_bytes);
    }

    pub(crate) fn usage(&self) -> CacheUsage {
        let mut usage = CacheUsage {
            cpu_bytes: self.bytes,
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            largest_entry_bytes: self
                .entries
                .values()
                .map(|entry| entry.bytes)
                .max()
                .unwrap_or(0),
            ..Default::default()
        };
        match self.class {
            ResourceClass::Live => usage.live_bytes = self.bytes,
            ResourceClass::Rebuildable => usage.rebuildable_bytes = self.bytes,
            ResourceClass::Cache => usage.cache_bytes = self.bytes,
            ResourceClass::Transient => {}
        }
        usage
    }

    fn evict_to(&mut self, target_bytes: usize) {
        while self.bytes > target_bytes {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
        self.publish();
    }

    fn publish(&self) {
        self.telemetry.publish(self.usage());
    }
}
