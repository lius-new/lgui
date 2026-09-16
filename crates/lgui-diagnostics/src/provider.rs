use std::sync::Arc;

use lgui_core::core::{HostTree, UiRect};

use super::{DiagnosticsQuery, FrameDiagnosticsSnapshot, FrameSample};

pub trait DiagnosticsProvider: Send + Sync {
    fn snapshot(&self) -> FrameDiagnosticsSnapshot;
    fn query(&self, query: DiagnosticsQuery) -> Vec<FrameSample>;
}

pub trait DiagnosticsSink: Send + Sync {
    fn record(&self, sample: FrameSample, tree: &HostTree, viewport: UiRect);
}

#[derive(Clone)]
#[doc(hidden)]
pub struct DiagnosticsRegistration {
    sink: Arc<dyn DiagnosticsSink>,
}

impl DiagnosticsRegistration {
    pub fn new(sink: impl DiagnosticsSink + 'static) -> Self {
        Self {
            sink: Arc::new(sink),
        }
    }

    pub fn record(&self, sample: FrameSample, tree: &HostTree, viewport: UiRect) {
        self.sink.record(sample, tree, viewport);
    }
}
