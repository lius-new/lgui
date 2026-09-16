//! Frame and system diagnostics contracts, samples, and collection.

mod collector;
mod model;
mod provider;
mod system;
mod timing;

pub use collector::FrameCollector;
pub use model::{
    DiagnosticPresentMode, DiagnosticsQuery, FrameBlitSourceMetrics, FrameDiagnosticsSnapshot,
    FramePresentMetrics, FrameRenderMetrics, FrameSample, GpuUsageSnapshot, MachineUsageSnapshot,
    ProcessUsageSnapshot, RendererDeviceInfo, SystemUsageSnapshot,
};
#[doc(hidden)]
pub use provider::DiagnosticsRegistration;
pub use provider::{DiagnosticsProvider, DiagnosticsSink};
pub use system::{system_usage_sample_interval_ms, system_usage_snapshot};
pub use timing::duration_ms;

#[path = "diagnostics_test.rs"]
#[cfg(test)]
mod tests;
