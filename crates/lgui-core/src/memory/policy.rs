use std::time::Duration;

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheDomain {
    EncodedImage,
    DecodedImage,
    Svg,
    Blur,
    Text,
    StaticLayer,
    ScrollRaster,
    Skia,
    ComponentOutput,
    HostScene,
    Diagnostics,
    Persistent,
}

impl CacheDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EncodedImage => "encoded-image",
            Self::DecodedImage => "decoded-image",
            Self::Svg => "svg",
            Self::Blur => "blur",
            Self::Text => "text",
            Self::StaticLayer => "static-layer",
            Self::ScrollRaster => "scroll-raster",
            Self::Skia => "skia",
            Self::ComponentOutput => "component-output",
            Self::HostScene => "host-scene",
            Self::Diagnostics => "diagnostics",
            Self::Persistent => "persistent",
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum ResourceClass {
    Live,
    Rebuildable,
    #[default]
    Cache,
    Transient,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetentionClass {
    Frame,
    WhileVisible,
    Scene,
    #[default]
    Session,
    Persistent,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum CachePriority {
    Low,
    #[default]
    Normal,
    High,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageCachePolicy {
    #[default]
    ApplicationDefault,
    NoStore,
    WhileVisible,
    Scene,
    Session,
    Persistent {
        max_age: Duration,
        revalidate: bool,
    },
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum CacheScope {
    #[default]
    Memory,
    Persistent,
    AllRebuildable,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum TrimReason {
    #[default]
    SoftBudget,
    HardBudget,
    WindowHidden,
    AllWindowsHidden,
    SessionUnmounted,
    DeviceLost,
    ThemeOrScaleChanged,
    ModeratePressure,
    CriticalPressure,
    Explicit,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum MemoryEvent {
    FrameCommitted,
    WindowHidden,
    WindowShown,
    AllWindowsHidden,
    SessionUnmounted,
    RendererDeviceLost,
    ThemeOrScaleChanged,
    ModeratePressure,
    CriticalPressure,
    ExplicitTrim,
    ApplicationShutdown,
}

impl MemoryEvent {
    pub const fn trim_reason(self) -> TrimReason {
        match self {
            Self::FrameCommitted => TrimReason::SoftBudget,
            Self::WindowHidden => TrimReason::WindowHidden,
            Self::WindowShown => TrimReason::SoftBudget,
            Self::AllWindowsHidden => TrimReason::AllWindowsHidden,
            Self::SessionUnmounted => TrimReason::SessionUnmounted,
            Self::RendererDeviceLost => TrimReason::DeviceLost,
            Self::ThemeOrScaleChanged => TrimReason::ThemeOrScaleChanged,
            Self::ModeratePressure => TrimReason::ModeratePressure,
            Self::CriticalPressure => TrimReason::CriticalPressure,
            Self::ExplicitTrim => TrimReason::Explicit,
            Self::ApplicationShutdown => TrimReason::Shutdown,
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum MemoryAction {
    #[default]
    None,
    EnforceBudget,
    Trim {
        scope: CacheScope,
        target_bytes: usize,
    },
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct CacheKey {
    pub namespace: String,
    pub key: String,
    pub version: u64,
}

impl CacheKey {
    pub fn new(namespace: impl Into<String>, key: impl Into<String>, version: u64) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
            version,
        }
    }
}
