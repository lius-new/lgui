use super::GpuUsageSnapshot;
use super::SystemUsageSnapshot;

pub fn system_usage_snapshot() -> SystemUsageSnapshot {
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
    0
}
