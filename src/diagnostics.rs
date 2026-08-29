use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::core::{HostTree, UiRect};

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

pub fn system_usage_snapshot() -> SystemUsageSnapshot {
    #[cfg(all(feature = "system-diagnostics", target_os = "windows"))]
    {
        return crate::platform::win32::snapshot_system_usage();
    }

    #[cfg(not(all(feature = "system-diagnostics", target_os = "windows")))]
    SystemUsageSnapshot {
        schema: "lgui.diagnostics.system-usage.v1",
        gpu: GpuUsageSnapshot {
            status: "unsupported",
            ..GpuUsageSnapshot::default()
        },
        ..SystemUsageSnapshot::default()
    }
}

pub fn system_usage_sample_interval_ms() -> u64 {
    #[cfg(all(feature = "system-diagnostics", target_os = "windows"))]
    {
        return crate::platform::win32::sample_interval_ms();
    }

    #[cfg(not(all(feature = "system-diagnostics", target_os = "windows")))]
    0
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
    pub runtime_reconcile_ms: f32,
    pub layout_ms: f32,
    pub host_commit_ms: f32,
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
    pub layout_visited_nodes: usize,
    pub layout_laid_out_nodes: usize,
    pub layout_reused_nodes: usize,
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
    pub get_dc_ms: f32,
    pub clear_ms: f32,
    pub draw_commands_ms: f32,
    pub submit_ms: f32,
    pub release_dc_ms: f32,
    pub submitted_pixels: u64,
    pub blit_count: usize,
    pub bitblt_count: usize,
    pub alphablend_count: usize,
    pub fallback_count: usize,
    pub blit_pixels: u64,
    pub static_layer_blits: FrameBlitSourceMetrics,
    pub overlay_blits: FrameBlitSourceMetrics,
    pub backdrop_blits: FrameBlitSourceMetrics,
    pub custom_blits: FrameBlitSourceMetrics,
    pub other_blits: FrameBlitSourceMetrics,
}

#[derive(Clone, Debug)]
pub struct FrameSample {
    pub frame_index: u64,
    pub recorded_at: Instant,
    pub backend: &'static str,
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

pub trait DiagnosticsProvider: Send + Sync {
    fn snapshot(&self) -> FrameDiagnosticsSnapshot;
    fn query(&self, query: DiagnosticsQuery) -> Vec<FrameSample>;
}

pub trait DiagnosticsSink: Send + Sync {
    fn record(&self, sample: FrameSample, tree: &HostTree, viewport: UiRect);
}

#[derive(Clone)]
pub(crate) struct DiagnosticsRegistration {
    sink: Arc<dyn DiagnosticsSink>,
}

impl DiagnosticsRegistration {
    pub(crate) fn new(sink: impl DiagnosticsSink + 'static) -> Self {
        Self {
            sink: Arc::new(sink),
        }
    }

    pub(crate) fn record(&self, sample: FrameSample, tree: &HostTree, viewport: UiRect) {
        self.sink.record(sample, tree, viewport);
    }
}

pub struct FrameCollector {
    capacity: usize,
    samples: VecDeque<FrameSample>,
}

impl FrameCollector {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            samples: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    pub fn record(&mut self, sample: FrameSample) {
        self.samples.push_back(sample);
        while self.samples.len() > self.capacity {
            self.samples.pop_front();
        }
    }

    pub fn snapshot(&self) -> FrameDiagnosticsSnapshot {
        let now = Instant::now();
        let sample_count = self.samples.len();
        let average_frame_ms = if sample_count == 0 {
            0.0
        } else {
            self.samples
                .iter()
                .map(|sample| sample.total_ms)
                .sum::<f32>()
                / sample_count as f32
        };
        FrameDiagnosticsSnapshot {
            fps: self
                .samples
                .iter()
                .filter(|sample| sample.mode != DiagnosticPresentMode::Skipped)
                .filter(|sample| {
                    now.saturating_duration_since(sample.recorded_at) <= Duration::from_secs(1)
                })
                .count() as f32,
            average_frame_ms,
            p95_frame_ms: percentile(self.samples.iter().map(|sample| sample.total_ms), 0.95),
            sample_count,
            latest: self.samples.back().cloned(),
        }
    }

    pub fn query(&self, query: DiagnosticsQuery) -> Vec<FrameSample> {
        let limit = query.recent_limit.min(self.samples.len());
        self.samples
            .iter()
            .skip(self.samples.len() - limit)
            .cloned()
            .collect()
    }
}

pub fn duration_ms(duration: Duration) -> f32 {
    duration.as_secs_f64() as f32 * 1000.0
}

fn percentile(values: impl Iterator<Item = f32>, percentile: f32) -> f32 {
    let mut values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    let index = ((values.len() - 1) as f32 * percentile.clamp(0.0, 1.0)).ceil() as usize;
    values[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingSink(Arc<AtomicUsize>);

    impl DiagnosticsSink for CountingSink {
        fn record(&self, _sample: FrameSample, _tree: &HostTree, _viewport: UiRect) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn sample(index: u64, total_ms: f32, mode: DiagnosticPresentMode) -> FrameSample {
        FrameSample {
            frame_index: index,
            recorded_at: Instant::now(),
            backend: "test",
            mode,
            frame_build_ms: 0.0,
            diff_ms: 0.0,
            draw_present_ms: 0.0,
            total_ms,
            dirty_rect_count: 0,
            dirty_area_ratio: 0.0,
            submit_scope: "test",
            fallback_reason: None,
            primary_reason: None,
            render: FrameRenderMetrics::default(),
            present: FramePresentMetrics::default(),
        }
    }

    #[test]
    fn collector_is_bounded_and_queries_recent_samples_in_order() {
        let mut collector = FrameCollector::new(2);
        collector.record(sample(1, 10.0, DiagnosticPresentMode::Full));
        collector.record(sample(2, 20.0, DiagnosticPresentMode::Dirty));
        collector.record(sample(3, 30.0, DiagnosticPresentMode::Skipped));

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.sample_count, 2);
        assert_eq!(snapshot.fps, 1.0);
        assert_eq!(snapshot.average_frame_ms, 25.0);
        assert_eq!(snapshot.p95_frame_ms, 30.0);
        assert_eq!(snapshot.latest.unwrap().frame_index, 3);
        assert_eq!(
            collector.query(DiagnosticsQuery::recent(1))[0].frame_index,
            3
        );
    }

    #[test]
    fn diagnostics_registration_forwards_the_current_tree_and_frame() {
        let count = Arc::new(AtomicUsize::new(0));
        let registration = DiagnosticsRegistration::new(CountingSink(Arc::clone(&count)));

        registration.record(
            sample(1, 2.0, DiagnosticPresentMode::Dirty),
            &HostTree::new(),
            UiRect::new(0, 0, 100, 80),
        );

        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
