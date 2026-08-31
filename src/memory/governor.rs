use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use super::registry::RegistryState;
use super::{
    CacheDomain, CacheRegistration, CacheScope, CacheUsage, DomainInstanceId, DomainRegistration,
    DomainSnapshot, MemoryEvent, MemoryOptions, MemorySnapshot, TrimReason, TrimRequest,
    TrimSnapshot,
};

#[cfg(feature = "persistent-cache")]
use super::PersistentCacheStore;

#[derive(Clone)]
pub struct MemoryGovernor {
    inner: Arc<MemoryGovernorInner>,
}

struct MemoryGovernorInner {
    options: Mutex<MemoryOptions>,
    registry: Arc<Mutex<RegistryState>>,
    epoch: AtomicU64,
    transient_reserved: Arc<AtomicUsize>,
    large_tasks_in_flight: Arc<AtomicUsize>,
    last_trim: Mutex<Option<TrimSnapshot>>,
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
        Self {
            inner: Arc::new(MemoryGovernorInner {
                options: Mutex::new(options),
                registry: Arc::new(Mutex::new(RegistryState::default())),
                epoch: AtomicU64::new(1),
                transient_reserved: Arc::new(AtomicUsize::new(0)),
                large_tasks_in_flight: Arc::new(AtomicUsize::new(0)),
                last_trim: Mutex::new(None),
                #[cfg(feature = "persistent-cache")]
                persistent,
            }),
        }
    }

    pub fn options(&self) -> MemoryOptions {
        *self.inner.options.lock().expect("memory options poisoned")
    }

    pub fn set_options(&self, options: MemoryOptions) {
        *self.inner.options.lock().expect("memory options poisoned") = options;
        self.bump_epoch();
        self.rebalance_budgets();
        self.enforce_soft_budget(TrimReason::SoftBudget);
        #[cfg(feature = "persistent-cache")]
        if let Some(store) = self.persistent_cache() {
            let _ = store.trim_to(options.budget.persistent_bytes);
        }
    }

    pub fn next_instance_id(&self) -> DomainInstanceId {
        let mut registry = self
            .inner
            .registry
            .lock()
            .expect("memory registry poisoned");
        let id = registry.next_instance;
        registry.next_instance = registry.next_instance.wrapping_add(1).max(1);
        DomainInstanceId(id)
    }

    pub fn register(&self, registration: DomainRegistration) -> CacheRegistration {
        let mut registry = self
            .inner
            .registry
            .lock()
            .expect("memory registry poisoned");
        let id = registry.next_registration;
        registry.next_registration = registry.next_registration.wrapping_add(1).max(1);
        registry.entries.insert(id, registration);
        drop(registry);
        self.rebalance_budgets();
        self.bump_epoch();
        CacheRegistration::new(id, Arc::downgrade(&self.inner.registry))
    }

    pub fn snapshot(&self) -> MemorySnapshot {
        let epoch = self.inner.epoch.load(Ordering::Acquire);
        let entries = self.registered_entries();
        let mut usage = CacheUsage::default();
        let mut domains = Vec::with_capacity(entries.len());
        for (id, registration) in entries {
            let domain_usage = registration.adapter.usage();
            usage.add_assign(domain_usage);
            domains.push(DomainSnapshot {
                registration_id: id,
                domain: registration.domain,
                instance: registration.instance,
                owner: registration.owner,
                usage: domain_usage,
            });
        }
        let options = self.options();
        let cache_soft = options
            .budget
            .cpu_cache_soft_bytes
            .saturating_add(options.budget.native_cache_soft_bytes);
        MemorySnapshot {
            epoch,
            options,
            transient_reserved_bytes: self.inner.transient_reserved.load(Ordering::Acquire),
            large_tasks_in_flight: self.inner.large_tasks_in_flight.load(Ordering::Acquire),
            pinned_overflow_bytes: usage.pinned_bytes.saturating_sub(cache_soft),
            usage,
            domains,
            last_trim: *self
                .inner
                .last_trim
                .lock()
                .expect("memory trim state poisoned"),
        }
    }

    pub fn trim(&self, reason: TrimReason, scope: CacheScope, target_bytes: usize) -> usize {
        let epoch = self.bump_epoch();
        let started = Instant::now();
        let entries = self.registered_entries();
        let mut released = 0usize;
        for (_, registration) in &entries {
            let assigned = self.assigned_budget(registration.domain, &entries);
            let domain_target = if target_bytes == 0 {
                0
            } else if target_bytes == usize::MAX {
                assigned
            } else {
                let total = self
                    .options()
                    .budget
                    .cpu_cache_soft_bytes
                    .saturating_add(self.options().budget.native_cache_soft_bytes)
                    .max(1);
                assigned.saturating_mul(target_bytes).saturating_div(total)
            };
            let result = registration.adapter.trim(TrimRequest {
                reason,
                scope,
                target_bytes: domain_target,
            });
            released = released.saturating_add(result.released_bytes());
        }
        let duration_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        *self
            .inner
            .last_trim
            .lock()
            .expect("memory trim state poisoned") = Some(TrimSnapshot {
            reason,
            requested_at_epoch: epoch,
            released_bytes: released,
            duration_micros,
        });
        released
    }

    pub fn invalidate_domain(&self, domain: CacheDomain) -> usize {
        let entries = self.registered_entries();
        let mut released = 0usize;
        for (_, registration) in entries {
            if registration.domain == domain {
                released = released.saturating_add(
                    registration
                        .adapter
                        .trim(TrimRequest {
                            reason: TrimReason::Explicit,
                            scope: CacheScope::Memory,
                            target_bytes: 0,
                        })
                        .released_bytes(),
                );
            }
        }
        self.bump_epoch();
        released
    }

    pub fn notify(&self, event: MemoryEvent) {
        match event {
            MemoryEvent::FrameCommitted => self.enforce_soft_budget(TrimReason::SoftBudget),
            MemoryEvent::WindowShown => {}
            MemoryEvent::WindowHidden => {
                self.trim(TrimReason::WindowHidden, CacheScope::Memory, usize::MAX);
            }
            MemoryEvent::AllWindowsHidden => {
                self.trim(TrimReason::AllWindowsHidden, CacheScope::AllRebuildable, 0);
            }
            MemoryEvent::SessionUnmounted => {
                self.trim(
                    TrimReason::SessionUnmounted,
                    CacheScope::AllRebuildable,
                    usize::MAX,
                );
            }
            MemoryEvent::RendererDeviceLost => {
                self.trim(TrimReason::DeviceLost, CacheScope::Memory, 0);
            }
            MemoryEvent::ThemeOrScaleChanged => {
                self.trim(TrimReason::ThemeOrScaleChanged, CacheScope::Memory, 0);
            }
            MemoryEvent::ModeratePressure => {
                let options = self.options();
                let target = options
                    .budget
                    .cpu_cache_soft_bytes
                    .saturating_add(options.budget.native_cache_soft_bytes)
                    .saturating_mul(3)
                    / 4;
                self.trim(TrimReason::ModeratePressure, CacheScope::Memory, target);
            }
            MemoryEvent::CriticalPressure => {
                self.trim(TrimReason::CriticalPressure, CacheScope::AllRebuildable, 0);
            }
            MemoryEvent::ExplicitTrim => {
                self.trim(TrimReason::Explicit, CacheScope::AllRebuildable, 0);
            }
            MemoryEvent::ApplicationShutdown => {
                self.trim(TrimReason::Shutdown, CacheScope::AllRebuildable, 0);
            }
        }
    }

    pub fn try_reserve(&self, bytes: usize) -> Option<MemoryReservation> {
        let hard = self.options().budget.transient_hard_bytes;
        let reserved = &self.inner.transient_reserved;
        let mut current = reserved.load(Ordering::Acquire);
        loop {
            let next = current.checked_add(bytes)?;
            if next > hard {
                self.trim(TrimReason::HardBudget, CacheScope::Memory, 0);
                return None;
            }
            match reserved.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    return Some(MemoryReservation {
                        bytes,
                        reserved: Arc::clone(reserved),
                    })
                }
                Err(actual) => current = actual,
            }
        }
    }

    pub fn try_reserve_task(&self, bytes: usize) -> Option<MemoryTaskReservation> {
        let limit = self.options().budget.max_parallel_large_tasks.max(1);
        let in_flight = &self.inner.large_tasks_in_flight;
        let mut current = in_flight.load(Ordering::Acquire);
        loop {
            if current >= limit {
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
            self.bump_epoch();
            return store.stats();
        }
        Ok(super::PersistentCacheStats::default())
    }

    fn enforce_soft_budget(&self, reason: TrimReason) {
        let snapshot = self.snapshot();
        let options = snapshot.options;
        let cpu_exceeded = snapshot.usage.cpu_bytes > options.budget.cpu_cache_soft_bytes;
        let native_exceeded =
            snapshot.usage.gpu_estimated_bytes > options.budget.native_cache_soft_bytes;
        if cpu_exceeded || native_exceeded {
            self.trim(
                reason,
                CacheScope::Memory,
                options
                    .budget
                    .cpu_cache_soft_bytes
                    .saturating_add(options.budget.native_cache_soft_bytes),
            );
        }
    }

    fn registered_entries(&self) -> Vec<(u64, DomainRegistration)> {
        self.inner
            .registry
            .lock()
            .expect("memory registry poisoned")
            .entries
            .iter()
            .map(|(id, registration)| (*id, registration.clone()))
            .collect()
    }

    fn rebalance_budgets(&self) {
        let entries = self.registered_entries();
        for (_, registration) in &entries {
            registration
                .adapter
                .set_budget(self.assigned_budget(registration.domain, &entries));
        }
    }

    fn assigned_budget(&self, domain: CacheDomain, entries: &[(u64, DomainRegistration)]) -> usize {
        let options = self.options();
        if domain == CacheDomain::Persistent {
            return options.budget.persistent_bytes.min(usize::MAX as u64) as usize;
        }
        let native = is_native_domain(domain);
        let total = if native {
            options.budget.native_cache_soft_bytes
        } else {
            options.budget.cpu_cache_soft_bytes
        };
        let total_weight = entries
            .iter()
            .filter(|(_, registration)| {
                registration.domain != CacheDomain::Persistent
                    && is_native_domain(registration.domain) == native
            })
            .map(|(_, registration)| domain_weight(registration.domain))
            .sum::<usize>()
            .max(1);
        total
            .saturating_mul(domain_weight(domain))
            .saturating_div(total_weight)
            .max(1)
    }

    fn bump_epoch(&self) -> u64 {
        self.inner.epoch.fetch_add(1, Ordering::AcqRel) + 1
    }
}

fn is_native_domain(domain: CacheDomain) -> bool {
    matches!(
        domain,
        CacheDomain::Gdi | CacheDomain::D2d | CacheDomain::Skia
    )
}

fn domain_weight(domain: CacheDomain) -> usize {
    match domain {
        CacheDomain::EncodedImage => 20,
        CacheDomain::DecodedImage => 20,
        CacheDomain::Svg => 5,
        CacheDomain::Blur => 10,
        CacheDomain::Text => 10,
        CacheDomain::StaticLayer => 15,
        CacheDomain::ScrollRaster => 10,
        CacheDomain::ComponentOutput => 5,
        CacheDomain::HostScene => 5,
        CacheDomain::Diagnostics => 1,
        CacheDomain::Gdi | CacheDomain::D2d | CacheDomain::Skia | CacheDomain::Persistent => 100,
    }
}

impl Default for MemoryGovernor {
    fn default() -> Self {
        Self::new(MemoryOptions::default())
    }
}

pub struct MemoryReservation {
    bytes: usize,
    reserved: Arc<AtomicUsize>,
}

pub struct MemoryTaskReservation {
    _reservation: MemoryReservation,
    in_flight: Arc<AtomicUsize>,
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
        self.reserved.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
