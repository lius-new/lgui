use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
};

use super::MemoryOptions;

#[cfg(feature = "persistent-cache")]
use super::PersistentCacheStore;

/// Hard safety limits for the image-loading pipeline. These are not tunable
/// policy knobs — they exist to stop a single huge or hostile image from
/// exhausting memory or thread resources.
const TRANSIENT_HARD_BYTES: usize = 512 * 1024 * 1024;
const MAX_PARALLEL_LARGE_TASKS: usize = 4;

/// Opaque owner id handed out by [`MemoryGovernor::next_instance_id`] for
/// image-cache reachability tracking.
#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct DomainInstanceId(pub u64);

/// Application-scoped memory governor.
///
/// Owns the application [`MemoryOptions`], hands out image-cache owner ids,
/// and enforces transient/large-task safety limits for the image pipeline.
#[derive(Clone)]
pub struct MemoryGovernor {
    inner: Arc<MemoryGovernorInner>,
}

struct MemoryGovernorInner {
    options: Mutex<MemoryOptions>,
    next_instance: AtomicU64,
    transient_reserved: Arc<AtomicUsize>,
    large_tasks_in_flight: Arc<AtomicUsize>,
    #[cfg(feature = "persistent-cache")]
    persistent: Option<Arc<dyn PersistentCacheStore>>,
}

impl MemoryGovernor {
    pub fn new(options: MemoryOptions) -> Self {
        Self::with_store(
            options,
            #[cfg(feature = "persistent-cache")]
            None,
        )
    }

    pub(crate) fn with_store(
        options: MemoryOptions,
        #[cfg(feature = "persistent-cache")] persistent: Option<Arc<dyn PersistentCacheStore>>,
    ) -> Self {
        options
            .validate()
            .expect("invalid application memory policy");
        Self {
            inner: Arc::new(MemoryGovernorInner {
                options: Mutex::new(options),
                next_instance: AtomicU64::new(1),
                transient_reserved: Arc::new(AtomicUsize::new(0)),
                large_tasks_in_flight: Arc::new(AtomicUsize::new(0)),
                #[cfg(feature = "persistent-cache")]
                persistent,
            }),
        }
    }

    pub fn options(&self) -> MemoryOptions {
        *self.inner.options.lock().expect("memory options poisoned")
    }

    pub fn set_options(&self, options: MemoryOptions) {
        options
            .validate()
            .expect("invalid application memory policy");
        *self.inner.options.lock().expect("memory options poisoned") = options;
        #[cfg(feature = "persistent-cache")]
        if let Some(store) = self.persistent_cache() {
            let _ = store.trim_to(options.budget.persistent_bytes);
        }
    }

    /// Allocates a unique owner id for image-cache reachability tracking.
    pub fn next_instance_id(&self) -> DomainInstanceId {
        let id = self.inner.next_instance.fetch_add(1, Ordering::AcqRel);
        DomainInstanceId(id)
    }

    /// Reserves `bytes` against the transient in-flight memory ceiling.
    pub fn try_reserve(&self, bytes: usize) -> Option<MemoryReservation> {
        let reserved = &self.inner.transient_reserved;
        let mut current = reserved.load(Ordering::Acquire);
        loop {
            let next = current.checked_add(bytes)?;
            if next > TRANSIENT_HARD_BYTES {
                return None;
            }
            match reserved.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(MemoryReservation {
                        bytes,
                        accounted_bytes: bytes,
                        reserved: Arc::clone(reserved),
                    })
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Reserves a large-task slot plus `bytes` of transient memory.
    pub fn try_reserve_task(&self, bytes: usize) -> Option<MemoryTaskReservation> {
        let in_flight = &self.inner.large_tasks_in_flight;
        let mut current = in_flight.load(Ordering::Acquire);
        loop {
            if current >= MAX_PARALLEL_LARGE_TASKS {
                return None;
            }
            match in_flight.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        let Some(reservation) = self.try_reserve(bytes) else {
            in_flight.fetch_sub(1, Ordering::AcqRel);
            return None;
        };
        Some(MemoryTaskReservation {
            _reservation: reservation,
            in_flight: Arc::clone(in_flight),
        })
    }

    #[cfg(feature = "persistent-cache")]
    pub fn persistent_cache(&self) -> Option<Arc<dyn PersistentCacheStore>> {
        self.options()
            .persistent_cache_enabled
            .then(|| self.inner.persistent.as_ref().map(Arc::clone))
            .flatten()
    }

    #[cfg(feature = "persistent-cache")]
    pub fn persistent_cache_stats(
        &self,
    ) -> Result<super::PersistentCacheStats, super::CacheStoreError> {
        self.inner.persistent.as_ref().map_or_else(
            || Ok(super::PersistentCacheStats::default()),
            |store| store.stats(),
        )
    }

    #[cfg(feature = "persistent-cache")]
    pub fn clear_persistent_cache(
        &self,
        namespace: Option<&str>,
    ) -> Result<super::PersistentCacheStats, super::CacheStoreError> {
        if let Some(store) = self.inner.persistent.as_ref() {
            store.clear(namespace)?;
            return store.stats();
        }
        Ok(super::PersistentCacheStats::default())
    }
}

pub struct MemoryReservation {
    bytes: usize,
    accounted_bytes: usize,
    reserved: Arc<AtomicUsize>,
}

pub struct MemoryTaskReservation {
    _reservation: MemoryReservation,
    in_flight: Arc<AtomicUsize>,
}

impl MemoryTaskReservation {
    pub const fn bytes(&self) -> usize {
        self._reservation.bytes()
    }
}

impl Drop for MemoryTaskReservation {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

impl MemoryReservation {
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.reserved
            .fetch_sub(self.accounted_bytes, Ordering::AcqRel);
    }
}
