const MIB: usize = 1024 * 1024;

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemoryProfile {
    LowMemory,
    #[default]
    Balanced,
    Performance,
}

impl MemoryProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LowMemory => "low-memory",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryBudget {
    pub cpu_cache_soft_bytes: usize,
    pub cpu_cache_hard_bytes: usize,
    pub native_cache_soft_bytes: usize,
    pub native_cache_hard_bytes: usize,
    pub transient_hard_bytes: usize,
    pub persistent_bytes: u64,
    pub max_encoded_resource_bytes: usize,
    pub max_decoded_resource_bytes: usize,
    pub max_parallel_large_tasks: usize,
}

impl MemoryBudget {
    pub const fn for_profile(profile: MemoryProfile) -> Self {
        let (cpu, native, transient, persistent, parallel) = match profile {
            MemoryProfile::LowMemory => (64 * MIB, 64 * MIB, 32 * MIB, 128 * MIB as u64, 1),
            MemoryProfile::Balanced => (128 * MIB, 128 * MIB, 64 * MIB, 512 * MIB as u64, 2),
            MemoryProfile::Performance => {
                (256 * MIB, 256 * MIB, 128 * MIB, 2 * 1024 * MIB as u64, 2)
            }
        };
        Self {
            cpu_cache_soft_bytes: cpu,
            cpu_cache_hard_bytes: hard_budget(cpu),
            native_cache_soft_bytes: native,
            native_cache_hard_bytes: hard_budget(native),
            transient_hard_bytes: transient,
            persistent_bytes: persistent,
            max_encoded_resource_bytes: 32 * MIB,
            max_decoded_resource_bytes: 64 * MIB,
            max_parallel_large_tasks: parallel,
        }
    }
}

const fn hard_budget(soft: usize) -> usize {
    let extra = soft / 4;
    soft + if extra < 64 * MIB { extra } else { 64 * MIB }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryOptions {
    pub profile: MemoryProfile,
    pub budget: MemoryBudget,
    pub persistent_cache_enabled: bool,
}

impl MemoryOptions {
    pub const fn for_profile(profile: MemoryProfile) -> Self {
        Self {
            profile,
            budget: MemoryBudget::for_profile(profile),
            persistent_cache_enabled: true,
        }
    }

    pub const fn low_memory() -> Self {
        Self::for_profile(MemoryProfile::LowMemory)
    }

    pub const fn balanced() -> Self {
        Self::for_profile(MemoryProfile::Balanced)
    }

    pub const fn performance() -> Self {
        Self::for_profile(MemoryProfile::Performance)
    }

    pub const fn persistent_cache(mut self, enabled: bool) -> Self {
        self.persistent_cache_enabled = enabled;
        self
    }

    pub const fn persistent_budget(mut self, bytes: u64) -> Self {
        self.budget.persistent_bytes = bytes;
        self
    }
}

impl Default for MemoryOptions {
    fn default() -> Self {
        Self::balanced()
    }
}
