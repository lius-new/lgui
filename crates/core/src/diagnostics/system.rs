#[cfg(not(all(feature = "system-diagnostics", target_os = "windows")))]
use super::GpuUsageSnapshot;
use super::SystemUsageSnapshot;

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
