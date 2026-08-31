use std::time::Instant;

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug, Default)]
pub struct SystemUsageSnapshot {
    pub schema: &'static str,
    pub sample_interval_ms: u64,
    pub sample_age_ms: u64,
    pub process: ProcessUsageSnapshot,
    pub system: MachineUsageSnapshot,
    pub gpu: GpuUsageSnapshot,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug, Default)]
pub struct ProcessUsageSnapshot {
    pub cpu_percent: Option<f32>,
    pub working_set_mb: Option<f32>,
    pub pagefile_mb: Option<f32>,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug, Default)]
pub struct MachineUsageSnapshot {
    pub memory_load_percent: Option<u32>,
    pub total_memory_mb: Option<f32>,
    pub available_memory_mb: Option<f32>,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug)]
pub struct GpuUsageSnapshot {
    pub usage_percent: Option<f32>,
    pub memory_mb: Option<f32>,
    pub status: &'static str,
}

impl Default for GpuUsageSnapshot {
    fn default() -> Self {
        Self {
            usage_percent: None,
            memory_mb: None,
            status: "not-sampled",
        }
    }
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticPresentMode {
    Full,
    Dirty,
    Skipped,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameRenderMetrics {
    pub build_host_tree_ms: f32,
    pub pending_updates_ms: f32,
    pub prepare_render_ms: f32,
    pub retained_snapshot_ms: f32,
    pub declarative_mount_ms: f32,
    pub focus_animation_sync_ms: f32,
    pub focus_sync_ms: f32,
    pub focus_rebuild_ms: f32,
    pub animation_target_sync_ms: f32,
    pub animation_rebuild_ms: f32,
    pub runtime_reconcile_ms: f32,
    pub layout_ms: f32,
    pub host_commit_ms: f32,
    pub host_change_scan_ms: f32,
    pub host_node_patch_ms: f32,
    pub host_scene_reconcile_ms: f32,
    pub host_scene_snapshot_ms: f32,
    pub host_damage_ms: f32,
    pub host_finalize_ms: f32,
    pub host_unattributed_ms: f32,
    pub scene_ms: f32,
    pub metadata_ms: f32,
    pub snapshot_ms: f32,
    pub render_total_ms: f32,
    pub node_count: usize,
    pub command_count: usize,
    pub component_executed: usize,
    pub component_dirty: usize,
    pub projection_visited_nodes: usize,
    pub projection_reused_component_roots: usize,
    pub animation_sync_nodes: usize,
    pub focus_sync_needed: bool,
    pub layout_visited_nodes: usize,
    pub layout_laid_out_nodes: usize,
    pub layout_reused_nodes: usize,
    pub host_visited_nodes: usize,
    pub scene_compiled_nodes: usize,
    pub host_mutations: usize,
    pub scene_mutations: usize,
    pub reused_scene_nodes: usize,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameBlitSourceMetrics {
    pub blit_count: usize,
    pub bitblt_count: usize,
    pub alphablend_count: usize,
    pub fallback_count: usize,
    pub pixels: u64,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Default)]
pub struct FramePresentMetrics {
    pub acquire_ms: f32,
    pub get_dc_ms: f32,
    pub clear_ms: f32,
    pub draw_commands_ms: f32,
    pub flush_ms: f32,
    pub submit_ms: f32,
    pub present_ms: f32,
    pub release_dc_ms: f32,
    pub submitted_pixels: u64,
    pub blit_count: usize,
    pub bitblt_count: usize,
    pub alphablend_count: usize,
    pub fallback_count: usize,
    pub cache_budget_bytes: usize,
    pub cache_resident_bytes: usize,
    pub cache_entries: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_evictions: u64,
    pub text_cache_resident_bytes: usize,
    pub text_cache_entries: usize,
    pub text_cache_hits: u64,
    pub text_cache_misses: u64,
    pub text_cache_evictions: u64,
    pub largest_cache_entry_bytes: usize,
    pub largest_text_cache_entry_bytes: usize,
    pub blit_pixels: u64,
    pub static_layer_blits: FrameBlitSourceMetrics,
    pub overlay_blits: FrameBlitSourceMetrics,
    pub backdrop_blits: FrameBlitSourceMetrics,
    pub custom_blits: FrameBlitSourceMetrics,
    pub other_blits: FrameBlitSourceMetrics,
}

#[cfg_attr(feature = "diagnostics-serde", derive(serde::Serialize))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RendererDeviceInfo {
    pub adapter_name: Option<String>,
    pub api: String,
    pub api_version: Option<String>,
    pub color_format: String,
    pub present_mode: String,
}

#[derive(Clone, Debug)]
pub struct FrameSample {
    pub frame_index: u64,
    pub recorded_at: Instant,
    pub backend: &'static str,
    pub renderer: RendererDeviceInfo,
    pub mode: DiagnosticPresentMode,
    pub frame_build_ms: f32,
    pub diff_ms: f32,
    pub draw_present_ms: f32,
    pub total_ms: f32,
    pub dirty_rect_count: usize,
    pub dirty_area_ratio: f32,
    pub submit_scope: &'static str,
    pub fallback_reason: Option<&'static str>,
    pub primary_reason: Option<&'static str>,
    pub recovery_state: &'static str,
    pub recovery_attempt: u8,
    pub render: FrameRenderMetrics,
    pub present: FramePresentMetrics,
}

#[derive(Clone, Debug, Default)]
pub struct FrameDiagnosticsSnapshot {
    pub fps: f32,
    pub average_frame_ms: f32,
    pub p95_frame_ms: f32,
    pub sample_count: usize,
    pub latest: Option<FrameSample>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticsQuery {
    pub recent_limit: usize,
}

impl DiagnosticsQuery {
    pub const fn recent(recent_limit: usize) -> Self {
        Self { recent_limit }
    }
}
