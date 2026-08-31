use std::hash::{Hash, Hasher};

use super::{geometry::normalized_f32_bits, ImageFit};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StaticLayerSource {
    BakedAsset {
        key: &'static str,
        fit: ImageFit,
    },
    RuntimeGenerated,
    Hybrid {
        baked_base: Option<&'static str>,
        fit: ImageFit,
    },
}

impl StaticLayerSource {
    pub fn baked(key: &'static str, fit: ImageFit) -> Self {
        Self::BakedAsset { key, fit }
    }

    pub fn runtime() -> Self {
        Self::RuntimeGenerated
    }

    pub fn hybrid(baked_base: Option<&'static str>, fit: ImageFit) -> Self {
        Self::Hybrid { baked_base, fit }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RasterCachePolicy {
    Disabled,
    Memory {
        retention: crate::memory::RetentionClass,
        priority: crate::memory::CachePriority,
    },
}

impl RasterCachePolicy {
    pub const fn memory(
        retention: crate::memory::RetentionClass,
        priority: crate::memory::CachePriority,
    ) -> Self {
        Self::Memory {
            retention,
            priority,
        }
    }

    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Memory { .. })
    }

    pub const fn retention(self) -> Option<crate::memory::RetentionClass> {
        match self {
            Self::Disabled => None,
            Self::Memory { retention, .. } => Some(retention),
        }
    }

    pub const fn priority(self) -> Option<crate::memory::CachePriority> {
        match self {
            Self::Disabled => None,
            Self::Memory { priority, .. } => Some(priority),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaticLayerBackground {
    Opaque,
    Transparent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticLayerSpec {
    pub source: StaticLayerSource,
    pub cache_policy: RasterCachePolicy,
    pub revision: &'static str,
    pub opacity: u8,
    pub offset_x: f32,
    pub offset_y: f32,
    pub memory_budget_bytes: usize,
    pub background: StaticLayerBackground,
}

impl StaticLayerSpec {
    pub fn new(source: StaticLayerSource) -> Self {
        Self {
            source,
            // Bitmap caching is opt-in. Static layers may contain dynamic children, so callers
            // must explicitly choose memory caching only for stable reusable content.
            cache_policy: RasterCachePolicy::Disabled,
            revision: "v1",
            opacity: 255,
            offset_x: 0.0,
            offset_y: 0.0,
            memory_budget_bytes: 64 * 1024 * 1024,
            background: StaticLayerBackground::Opaque,
        }
    }

    pub fn cache_policy(mut self, policy: RasterCachePolicy) -> Self {
        self.cache_policy = policy;
        self
    }

    pub fn revision(mut self, revision: &'static str) -> Self {
        self.revision = revision;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        self
    }

    pub fn opacity_f32(&self) -> f32 {
        self.opacity as f32 / 255.0
    }

    pub fn paint_offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub fn memory_budget_bytes(mut self, budget: usize) -> Self {
        self.memory_budget_bytes = budget.max(1);
        self
    }

    pub fn background(mut self, background: StaticLayerBackground) -> Self {
        self.background = background;
        self
    }

    pub fn transparent_background(mut self) -> Self {
        self.background = StaticLayerBackground::Transparent;
        self
    }

    pub fn cache_signature(&self) -> StaticLayerCacheSignature {
        StaticLayerCacheSignature {
            source: self.source.clone(),
            cache_policy: self.cache_policy,
            revision: self.revision,
            background: self.background,
        }
    }
}

impl Eq for StaticLayerSpec {}

impl Hash for StaticLayerSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.cache_policy.hash(state);
        self.revision.hash(state);
        self.opacity.hash(state);
        normalized_f32_bits(self.offset_x).hash(state);
        normalized_f32_bits(self.offset_y).hash(state);
        self.memory_budget_bytes.hash(state);
        self.background.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StaticLayerCacheSignature {
    source: StaticLayerSource,
    cache_policy: RasterCachePolicy,
    revision: &'static str,
    background: StaticLayerBackground,
}
