use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use super::registry::RegistryState;
use super::{
    CacheDomain, CacheRegistration, CacheScope, CacheUsage, DomainInstanceId, DomainRegistration,
    DomainSnapshot, MemoryAction, MemoryEvent, MemoryOptions, MemorySnapshot, TrimReason,
    TrimRequest, TrimSnapshot,
};

#[cfg(feature = "persistent-cache")]
use super::PersistentCacheStore;

const FRAME_BUDGET_CHECK_INTERVAL: Duration = Duration::from_millis(250);

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
    last_frame_budget_check: Mutex<Option<Instant>>,
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
                registry: Arc::new(Mutex::new(RegistryState::default())),
                epoch: AtomicU64::new(1),
                transient_reserved: Arc::new(AtomicUsize::new(0)),
                large_tasks_in_flight: Arc::new(AtomicUsize::new(0)),
                last_trim: Mutex::new(None),
                last_frame_budget_check: Mutex::new(None),
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
        self.bump_epoch();
        self.rebalance_budgets();
        self.enforce_budget(true);
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
        let governor = self.clone();
        CacheRegistration::new(id, Arc::downgrade(&self.inner.registry), move || {
            governor.rebalance_budgets();
            governor.bump_epoch();
        })
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
        let cache_soft = options.budget.cache_soft_bytes;
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
        let entries = if scope == CacheScope::Persistent {
            Vec::new()
        } else {
            self.registered_entries()
        };
        let assignments = entries
            .iter()
            .filter(|(_, registration)| {
                registration.domain != CacheDomain::Persistent
                    && (scope == CacheScope::AllRebuildable
                        || registration.domain != CacheDomain::HostScene)
            })
            .map(|(_, registration)| {
                (
                    registration,
                    self.assigned_budget(registration.domain, &entries),
                )
            })
            .collect::<Vec<_>>();
        let assigned_total = assignments
            .iter()
            .map(|(_, assigned)| *assigned as u128)
            .sum::<u128>();
        let mut released = 0usize;
        for (registration, assigned) in assignments {
            let domain_target = if target_bytes == 0 {
                0
            } else if target_bytes == usize::MAX {
                assigned
            } else if assigned_total == 0 {
                0
            } else {
                let proportional =
                    (assigned as u128).saturating_mul(target_bytes as u128) / assigned_total;
                usize::try_from(proportional).unwrap_or(usize::MAX)
            };
            let result = registration.adapter.trim(TrimRequest {
                reason,
                scope,
                target_bytes: domain_target,
            });
            released = released.saturating_add(result.released_bytes());
        }
        #[cfg(feature = "persistent-cache")]
        if scope == CacheScope::Persistent {
            if let Some(store) = self.inner.persistent.as_ref() {
                let target = u64::try_from(target_bytes).unwrap_or(u64::MAX);
                if let (Ok(before), Ok(after)) = (store.stats(), store.trim_to(target)) {
                    released = released.saturating_add(
                        usize::try_from(before.bytes.saturating_sub(after.bytes))
                            .unwrap_or(usize::MAX),
                    );
                }
            }
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
        match self.options().event_action(event) {
            MemoryAction::None => {}
            MemoryAction::EnforceBudget => {
                if event == MemoryEvent::FrameCommitted {
                    if self.begin_frame_budget_check() {
                        self.finish_frame_budget_check();
                    }
                } else {
                    self.enforce_budget(true);
                }
            }
            MemoryAction::Trim {
                scope,
                target_bytes,
            } => {
                self.trim(event.trim_reason(), scope, target_bytes);
            }
        }
    }

    pub fn try_reserve(&self, bytes: usize) -> Option<MemoryReservation> {
        let hard = self.options().budget.transient_hard_bytes;
        let reserved = &self.inner.transient_reserved;
        if hard == usize::MAX {
            return Some(MemoryReservation {
                bytes,
                accounted_bytes: 0,
                reserved: Arc::clone(reserved),
            });
        }
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
                        accounted_bytes: bytes,
                        reserved: Arc::clone(reserved),
                    })
                }
                Err(actual) => current = actual,
            }
        }
    }

    pub fn try_reserve_task(&self, bytes: usize) -> Option<MemoryTaskReservation> {
        let limit = self.options().budget.max_parallel_large_tasks;
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

    fn enforce_budget(&self, include_soft_limit: bool) {
        let options = self.options();
        let usage = self.budget_usage();
        let evictable = usage.managed_bytes.saturating_sub(usage.protected_bytes);
        let reason = if evictable > options.budget.cache_hard_bytes {
            Some(TrimReason::HardBudget)
        } else if include_soft_limit && evictable > options.budget.cache_soft_bytes {
            Some(TrimReason::SoftBudget)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.trim(reason, CacheScope::Memory, options.budget.cache_soft_bytes);
        }
    }

    pub(crate) fn begin_frame_budget_check(&self) -> bool {
        if self.options().event_action(MemoryEvent::FrameCommitted) != MemoryAction::EnforceBudget {
            return false;
        }
        let now = Instant::now();
        let mut last = self
            .inner
            .last_frame_budget_check
            .lock()
            .expect("memory budget check state poisoned");
        if last.is_some_and(|previous| {
            now.saturating_duration_since(previous) < FRAME_BUDGET_CHECK_INTERVAL
        }) {
            return false;
        }
        *last = Some(now);
        true
    }

    pub(crate) fn finish_frame_budget_check(&self) {
        self.enforce_budget(false);
    }

    fn budget_usage(&self) -> BudgetUsage {
        let probes = self.registered_usage_probes();
        let mut result = BudgetUsage::default();
        for (domain, adapter) in probes {
            let usage = adapter.usage();
            let managed = usage.managed_bytes();
            result.managed_bytes = result.managed_bytes.saturating_add(managed);
            let protected = if domain == CacheDomain::HostScene {
                managed
            } else {
                usage.pinned_bytes.min(managed)
            };
            result.protected_bytes = result.protected_bytes.saturating_add(protected);
        }
        result
    }

    fn registered_usage_probes(&self) -> Vec<(CacheDomain, super::CacheAdapter)> {
        self.inner
            .registry
            .lock()
            .expect("memory registry poisoned")
            .entries
            .values()
            .map(|registration| (registration.domain, registration.adapter.clone()))
            .collect()
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
        let instances = entries
            .iter()
            .filter(|(_, registration)| registration.domain == domain)
            .count()
            .max(1);
        options.domain_budget(domain).saturating_div(instances)
    }

    fn bump_epoch(&self) -> u64 {
        self.inner.epoch.fetch_add(1, Ordering::AcqRel) + 1
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BudgetUsage {
    managed_bytes: usize,
    protected_bytes: usize,
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
