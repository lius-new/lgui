use super::{CacheDomain, CacheScope, ImageCachePolicy, MemoryAction, MemoryEvent};

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryBudget {
    pub cache_soft_bytes: usize,
    pub cache_hard_bytes: usize,
    pub transient_hard_bytes: usize,
    pub persistent_bytes: u64,
    pub max_encoded_resource_bytes: usize,
    pub max_decoded_resource_bytes: usize,
    pub max_parallel_large_tasks: usize,
}

impl MemoryBudget {
    pub const fn new(
        cache_soft_bytes: usize,
        cache_hard_bytes: usize,
        transient_hard_bytes: usize,
        persistent_bytes: u64,
        max_encoded_resource_bytes: usize,
        max_decoded_resource_bytes: usize,
        max_parallel_large_tasks: usize,
    ) -> Self {
        Self {
            cache_soft_bytes,
            cache_hard_bytes,
            transient_hard_bytes,
            persistent_bytes,
            max_encoded_resource_bytes,
            max_decoded_resource_bytes,
            max_parallel_large_tasks,
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryDomainBudgets {
    pub encoded_image_bytes: usize,
    pub decoded_image_bytes: usize,
    pub svg_bytes: usize,
    pub blur_bytes: usize,
    pub text_bytes: usize,
    pub static_layer_bytes: usize,
    pub scroll_raster_bytes: usize,
    pub gdi_bytes: usize,
    pub d2d_bytes: usize,
    pub skia_bytes: usize,
    pub component_output_bytes: usize,
    pub host_scene_bytes: usize,
    pub diagnostics_bytes: usize,
}

impl MemoryDomainBudgets {
    pub const fn budget_for(self, domain: CacheDomain) -> usize {
        match domain {
            CacheDomain::EncodedImage => self.encoded_image_bytes,
            CacheDomain::DecodedImage => self.decoded_image_bytes,
            CacheDomain::Svg => self.svg_bytes,
            CacheDomain::Blur => self.blur_bytes,
            CacheDomain::Text => self.text_bytes,
            CacheDomain::StaticLayer => self.static_layer_bytes,
            CacheDomain::ScrollRaster => self.scroll_raster_bytes,
            CacheDomain::Gdi => self.gdi_bytes,
            CacheDomain::D2d => self.d2d_bytes,
            CacheDomain::Skia => self.skia_bytes,
            CacheDomain::ComponentOutput => self.component_output_bytes,
            CacheDomain::HostScene => self.host_scene_bytes,
            CacheDomain::Diagnostics => self.diagnostics_bytes,
            CacheDomain::Persistent => 0,
        }
    }

    pub const fn unbounded() -> Self {
        Self {
            encoded_image_bytes: usize::MAX,
            decoded_image_bytes: usize::MAX,
            svg_bytes: usize::MAX,
            blur_bytes: usize::MAX,
            text_bytes: usize::MAX,
            static_layer_bytes: usize::MAX,
            scroll_raster_bytes: usize::MAX,
            gdi_bytes: usize::MAX,
            d2d_bytes: usize::MAX,
            skia_bytes: usize::MAX,
            component_output_bytes: usize::MAX,
            host_scene_bytes: usize::MAX,
            diagnostics_bytes: usize::MAX,
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryEventPolicy {
    pub frame_committed: MemoryAction,
    pub window_hidden: MemoryAction,
    pub window_shown: MemoryAction,
    pub all_windows_hidden: MemoryAction,
    pub session_unmounted: MemoryAction,
    pub renderer_device_lost: MemoryAction,
    pub theme_or_scale_changed: MemoryAction,
    pub moderate_pressure: MemoryAction,
    pub critical_pressure: MemoryAction,
    pub explicit_trim: MemoryAction,
    pub application_shutdown: MemoryAction,
}

impl MemoryEventPolicy {
    pub const fn action_for(self, event: MemoryEvent) -> MemoryAction {
        match event {
            MemoryEvent::FrameCommitted => self.frame_committed,
            MemoryEvent::WindowHidden => self.window_hidden,
            MemoryEvent::WindowShown => self.window_shown,
            MemoryEvent::AllWindowsHidden => self.all_windows_hidden,
            MemoryEvent::SessionUnmounted => self.session_unmounted,
            MemoryEvent::RendererDeviceLost => self.renderer_device_lost,
            MemoryEvent::ThemeOrScaleChanged => self.theme_or_scale_changed,
            MemoryEvent::ModeratePressure => self.moderate_pressure,
            MemoryEvent::CriticalPressure => self.critical_pressure,
            MemoryEvent::ExplicitTrim => self.explicit_trim,
            MemoryEvent::ApplicationShutdown => self.application_shutdown,
        }
    }

    pub const fn ignore_all() -> Self {
        Self {
            frame_committed: MemoryAction::None,
            window_hidden: MemoryAction::None,
            window_shown: MemoryAction::None,
            all_windows_hidden: MemoryAction::None,
            session_unmounted: MemoryAction::None,
            renderer_device_lost: MemoryAction::None,
            theme_or_scale_changed: MemoryAction::None,
            moderate_pressure: MemoryAction::None,
            critical_pressure: MemoryAction::None,
            explicit_trim: MemoryAction::None,
            application_shutdown: MemoryAction::None,
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryOptions {
    pub policy_name: &'static str,
    pub budget: MemoryBudget,
    pub domains: MemoryDomainBudgets,
    pub events: MemoryEventPolicy,
    pub default_image_cache_policy: ImageCachePolicy,
    pub persistent_cache_enabled: bool,
}

impl MemoryOptions {
    pub const fn new(
        policy_name: &'static str,
        budget: MemoryBudget,
        domains: MemoryDomainBudgets,
        events: MemoryEventPolicy,
        default_image_cache_policy: ImageCachePolicy,
        persistent_cache_enabled: bool,
    ) -> Self {
        Self {
            policy_name,
            budget,
            domains,
            events,
            default_image_cache_policy,
            persistent_cache_enabled,
        }
    }

    pub const fn unbounded(
        default_image_cache_policy: ImageCachePolicy,
        persistent_cache_enabled: bool,
    ) -> Self {
        Self::new(
            "unbounded",
            MemoryBudget::new(
                usize::MAX,
                usize::MAX,
                usize::MAX,
                u64::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
            ),
            MemoryDomainBudgets::unbounded(),
            MemoryEventPolicy::ignore_all(),
            default_image_cache_policy,
            persistent_cache_enabled,
        )
    }

    pub const fn persistent_cache(mut self, enabled: bool) -> Self {
        self.persistent_cache_enabled = enabled;
        self
    }

    pub const fn persistent_budget(mut self, bytes: u64) -> Self {
        self.budget.persistent_bytes = bytes;
        self
    }

    pub fn validate(self) -> Result<(), &'static str> {
        if self.policy_name.is_empty() {
            return Err("memory policy name must not be empty");
        }
        if self.budget.cache_soft_bytes > self.budget.cache_hard_bytes {
            return Err("memory cache soft budget exceeds hard budget");
        }
        if self.budget.max_parallel_large_tasks == 0 {
            return Err("memory policy must allow at least one large task");
        }
        if self.budget.max_encoded_resource_bytes > self.budget.transient_hard_bytes {
            return Err("encoded resource limit exceeds transient hard budget");
        }
        if self.budget.max_decoded_resource_bytes > self.budget.transient_hard_bytes {
            return Err("decoded resource limit exceeds transient hard budget");
        }
        if self.default_image_cache_policy == ImageCachePolicy::ApplicationDefault {
            return Err("application default image policy must be concrete");
        }
        Ok(())
    }

    pub const fn event_action(self, event: MemoryEvent) -> MemoryAction {
        self.events.action_for(event)
    }

    pub fn domain_budget(self, domain: CacheDomain) -> usize {
        match domain {
            CacheDomain::Persistent => self.budget.persistent_bytes.min(usize::MAX as u64) as usize,
            domain => self.domains.budget_for(domain),
        }
    }
}

impl MemoryAction {
    pub const fn trim(scope: CacheScope, target_bytes: usize) -> Self {
        Self::Trim {
            scope,
            target_bytes,
        }
    }
}
