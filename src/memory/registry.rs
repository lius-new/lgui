use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex, Weak},
};

use super::{CacheDomain, CacheScope, CacheUsage, TrimReason};

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct DomainInstanceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrimRequest {
    pub reason: TrimReason,
    pub scope: CacheScope,
    pub target_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrimResult {
    pub before_bytes: usize,
    pub after_bytes: usize,
}

impl TrimResult {
    pub const fn released_bytes(self) -> usize {
        self.before_bytes.saturating_sub(self.after_bytes)
    }
}

#[derive(Clone)]
pub struct CacheAdapter {
    usage: Arc<dyn Fn() -> CacheUsage + Send + Sync>,
    trim: Arc<dyn Fn(TrimRequest) -> TrimResult + Send + Sync>,
    set_budget: Arc<dyn Fn(usize) + Send + Sync>,
}

impl CacheAdapter {
    pub fn new(
        usage: impl Fn() -> CacheUsage + Send + Sync + 'static,
        trim: impl Fn(TrimRequest) -> TrimResult + Send + Sync + 'static,
    ) -> Self {
        Self {
            usage: Arc::new(usage),
            trim: Arc::new(trim),
            set_budget: Arc::new(|_| {}),
        }
    }

    pub fn managed(
        usage: impl Fn() -> CacheUsage + Send + Sync + 'static,
        trim: impl Fn(TrimRequest) -> TrimResult + Send + Sync + 'static,
        set_budget: impl Fn(usize) + Send + Sync + 'static,
    ) -> Self {
        Self {
            usage: Arc::new(usage),
            trim: Arc::new(trim),
            set_budget: Arc::new(set_budget),
        }
    }

    pub(crate) fn usage(&self) -> CacheUsage {
        (self.usage)()
    }

    pub(crate) fn trim(&self, request: TrimRequest) -> TrimResult {
        (self.trim)(request)
    }

    pub(crate) fn set_budget(&self, budget_bytes: usize) {
        (self.set_budget)(budget_bytes.max(1));
    }
}

impl fmt::Debug for CacheAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CacheAdapter(..)")
    }
}

#[derive(Clone, Debug)]
pub struct DomainRegistration {
    pub domain: CacheDomain,
    pub instance: DomainInstanceId,
    pub owner: String,
    pub adapter: CacheAdapter,
}

impl DomainRegistration {
    pub fn new(
        domain: CacheDomain,
        instance: DomainInstanceId,
        owner: impl Into<String>,
        adapter: CacheAdapter,
    ) -> Self {
        Self {
            domain,
            instance,
            owner: owner.into(),
            adapter,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RegistryState {
    pub next_registration: u64,
    pub next_instance: u64,
    pub entries: BTreeMap<u64, DomainRegistration>,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            next_registration: 1,
            next_instance: 1,
            entries: BTreeMap::new(),
        }
    }
}

pub struct CacheRegistration {
    id: u64,
    registry: Weak<Mutex<RegistryState>>,
}

impl CacheRegistration {
    pub(crate) fn new(id: u64, registry: Weak<Mutex<RegistryState>>) -> Self {
        Self { id, registry }
    }

    pub const fn id(&self) -> u64 {
        self.id
    }
}

impl fmt::Debug for CacheRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CacheRegistration")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for CacheRegistration {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry
                .lock()
                .expect("memory registry poisoned")
                .entries
                .remove(&self.id);
        }
    }
}
