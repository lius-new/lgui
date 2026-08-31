use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use super::{DiagnosticPresentMode, DiagnosticsQuery, FrameDiagnosticsSnapshot, FrameSample};

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

fn percentile(values: impl Iterator<Item = f32>, percentile: f32) -> f32 {
    let mut values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    let index = ((values.len() - 1) as f32 * percentile.clamp(0.0, 1.0)).ceil() as usize;
    values[index]
}
