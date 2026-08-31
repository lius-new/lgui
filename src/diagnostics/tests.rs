use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

use crate::core::{HostTree, UiRect};

use super::*;

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
        renderer: RendererDeviceInfo::default(),
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
        recovery_state: "healthy",
        recovery_attempt: 0,
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
        UiRect::new(0.0, 0.0, 100.0, 80.0),
    );

    assert_eq!(count.load(Ordering::SeqCst), 1);
}
