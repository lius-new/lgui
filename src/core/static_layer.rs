use super::ImageFit;

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
pub enum StaticLayerCachePolicy {
    Disabled,
    Memory,
    MemoryAndDisk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaticLayerBackground {
    Opaque,
    Transparent,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StaticLayerSpec {
    pub source: StaticLayerSource,
    pub cache_policy: StaticLayerCachePolicy,
    pub revision: &'static str,
    pub opacity: u8,
    pub offset_x: i32,
    pub offset_y: i32,
    pub memory_budget_bytes: usize,
    pub background: StaticLayerBackground,
}

impl StaticLayerSpec {
    pub fn new(source: StaticLayerSource) -> Self {
        Self {
            source,
            // Bitmap caching is opt-in. Static layers may contain dynamic children, so callers
            // must explicitly choose memory or disk caching only for stable reusable content.
            cache_policy: StaticLayerCachePolicy::Disabled,
            revision: "v1",
            opacity: 255,
            offset_x: 0,
            offset_y: 0,
            memory_budget_bytes: 64 * 1024 * 1024,
            background: StaticLayerBackground::Opaque,
        }
    }

    pub fn cache_policy(mut self, policy: StaticLayerCachePolicy) -> Self {
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

    pub fn paint_offset(mut self, x: i32, y: i32) -> Self {
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StaticLayerCacheSignature {
    source: StaticLayerSource,
    cache_policy: StaticLayerCachePolicy,
    revision: &'static str,
    background: StaticLayerBackground,
}
