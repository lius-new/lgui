use std::{
    mem::size_of,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::FILETIME,
    System::{
        ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        },
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes},
    },
};

use lgui_core::diagnostics::{
    GpuUsageSnapshot, MachineUsageSnapshot, ProcessUsageSnapshot, SystemUsageSnapshot,
};

#[derive(Default)]
struct CpuSampleState {
    process_time: u64,
    system_time: u64,
    value: Option<f32>,
}

#[derive(Clone)]
struct CachedSystemUsage {
    sampled_at: Instant,
    snapshot: SystemUsageSnapshot,
}

#[derive(Clone, Copy)]
struct ProcessMemory {
    working_set_bytes: usize,
    private_bytes: usize,
}

pub fn system_usage_snapshot() -> SystemUsageSnapshot {
    let interval = sample_interval();
    let mut cache = usage_cache().lock().expect("system usage cache poisoned");
    let now = Instant::now();
    if let Some(cached) = cache.as_ref() {
        if now.duration_since(cached.sampled_at) < interval {
            let mut snapshot = cached.snapshot.clone();
            snapshot.sample_age_ms = now.duration_since(cached.sampled_at).as_millis() as u64;
            return snapshot;
        }
    }

    let snapshot = sample_now(interval);
    *cache = Some(CachedSystemUsage {
        sampled_at: now,
        snapshot: snapshot.clone(),
    });
    snapshot
}

pub fn system_usage_sample_interval_ms() -> u64 {
    sample_interval().as_millis() as u64
}

fn sample_now(interval: Duration) -> SystemUsageSnapshot {
    let memory = process_memory();
    SystemUsageSnapshot {
        schema: "lgui.diagnostics.system-usage.v2",
        sample_interval_ms: interval.as_millis() as u64,
        sample_age_ms: 0,
        process: ProcessUsageSnapshot {
            cpu_percent: process_cpu_percent(),
            working_set_mb: memory.map(|value| bytes_to_mb(value.working_set_bytes)),
            private_mb: memory.map(|value| bytes_to_mb(value.private_bytes)),
        },
        system: system_memory(),
        gpu: GpuUsageSnapshot::default(),
    }
}

fn usage_cache() -> &'static Mutex<Option<CachedSystemUsage>> {
    static CACHE: OnceLock<Mutex<Option<CachedSystemUsage>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn sample_interval() -> Duration {
    static INTERVAL: OnceLock<Duration> = OnceLock::new();
    *INTERVAL.get_or_init(|| {
        let millis = std::env::var("LGUI_DIAGNOSTICS_SYSTEM_INTERVAL_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(1000)
            .clamp(250, 10_000);
        Duration::from_millis(millis)
    })
}

fn process_cpu_percent() -> Option<f32> {
    let (process_time, system_time) = process_and_system_times()?;
    let mut state = cpu_state().lock().expect("cpu usage state poisoned");
    if state.process_time == 0 || state.system_time == 0 {
        state.process_time = process_time;
        state.system_time = system_time;
        return state.value;
    }

    let process_delta = process_time.saturating_sub(state.process_time);
    let system_delta = system_time.saturating_sub(state.system_time);
    state.process_time = process_time;
    state.system_time = system_time;
    if system_delta == 0 {
        return state.value;
    }

    let percent = process_delta as f32 * 100.0 / system_delta as f32;
    state.value = Some(percent.clamp(0.0, 100.0));
    state.value
}

fn process_and_system_times() -> Option<(u64, u64)> {
    unsafe {
        let process = GetCurrentProcess();
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut process_kernel = FILETIME::default();
        let mut process_user = FILETIME::default();
        GetProcessTimes(
            process,
            &mut creation,
            &mut exit,
            &mut process_kernel,
            &mut process_user,
        )
        .ok()?;

        let mut system_idle = FILETIME::default();
        let mut system_kernel = FILETIME::default();
        let mut system_user = FILETIME::default();
        GetSystemTimes(
            Some(&mut system_idle),
            Some(&mut system_kernel),
            Some(&mut system_user),
        )
        .ok()?;

        Some((
            filetime_to_u64(process_kernel).saturating_add(filetime_to_u64(process_user)),
            filetime_to_u64(system_kernel).saturating_add(filetime_to_u64(system_user)),
        ))
    }
}

fn process_memory() -> Option<ProcessMemory> {
    unsafe {
        let process = GetCurrentProcess();
        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        GetProcessMemoryInfo(
            process,
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
        .ok()?;
        Some(ProcessMemory {
            working_set_bytes: counters.WorkingSetSize,
            private_bytes: counters.PrivateUsage,
        })
    }
}

fn system_memory() -> MachineUsageSnapshot {
    unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut status).is_err() {
            return MachineUsageSnapshot::default();
        }
        MachineUsageSnapshot {
            memory_load_percent: Some(status.dwMemoryLoad),
            total_memory_mb: Some(bytes_to_mb(status.ullTotalPhys as usize)),
            available_memory_mb: Some(bytes_to_mb(status.ullAvailPhys as usize)),
        }
    }
}

fn cpu_state() -> &'static Mutex<CpuSampleState> {
    static STATE: OnceLock<Mutex<CpuSampleState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CpuSampleState::default()))
}

fn filetime_to_u64(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

fn bytes_to_mb(bytes: usize) -> f32 {
    bytes as f32 / (1024.0 * 1024.0)
}
