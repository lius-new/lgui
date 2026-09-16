use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HMONITOR, MONITORINFO,
        MONITOR_DEFAULTTONEAREST,
    },
    UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
};

use crate::{
    core::{PhysicalPoint, PhysicalRect, PhysicalSize, Size, UiScale},
    platform::dpi::{ScaleContext, ScalePreference, WorkArea, BASE_DPI},
};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static SCALE_PREFERENCE: AtomicU32 = AtomicU32::new(0);

pub fn set_scale_preference(preference: ScalePreference) {
    let encoded = match preference {
        ScalePreference::Auto => 0,
        ScalePreference::Multiplier(value) => value.max(0.1).to_bits(),
    };
    SCALE_PREFERENCE.store(encoded, Ordering::Release);
}

fn scale_preference() -> ScalePreference {
    match SCALE_PREFERENCE.load(Ordering::Acquire) {
        0 => ScalePreference::Auto,
        encoded => ScalePreference::Multiplier(f32::from_bits(encoded)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DpiContext {
    pub dpi: u32,
    pub os_scale: f32,
    pub preference: ScalePreference,
    pub scale: UiScale,
    pub work_area: WorkArea,
    pub preferred_logical_size: Size,
    pub physical_window_size: PhysicalSize,
    pub compact: bool,
    pub generation: u64,
}

impl DpiContext {
    pub fn for_window(hwnd: HWND, preferred_logical_size: Size) -> Self {
        Self::for_window_with_preference(hwnd, preferred_logical_size, scale_preference())
    }

    pub fn for_window_system_scale(hwnd: HWND, preferred_logical_size: Size) -> Self {
        Self::for_window_with_preference(
            hwnd,
            preferred_logical_size,
            ScalePreference::Multiplier(1.0),
        )
    }

    fn for_window_with_preference(
        hwnd: HWND,
        preferred_logical_size: Size,
        preference: ScalePreference,
    ) -> Self {
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(BASE_DPI);
        Self::resolve(
            dpi,
            work_area_for_window(hwnd),
            preferred_logical_size,
            preference,
        )
    }

    pub fn for_point(point: PhysicalPoint, dpi: u32, preferred_logical_size: Size) -> Self {
        Self::resolve(
            dpi,
            work_area_for_point(point),
            preferred_logical_size,
            scale_preference(),
        )
    }

    pub fn for_monitor_point_system_scale(
        point: PhysicalPoint,
        preferred_logical_size: Size,
    ) -> Self {
        let monitor = monitor_for_point(point);
        Self::resolve(
            dpi_for_monitor(monitor),
            work_area_for_monitor(monitor),
            preferred_logical_size,
            ScalePreference::Multiplier(1.0),
        )
    }

    pub fn resolve(
        dpi: u32,
        work_area: WorkArea,
        preferred_logical_size: Size,
        preference: ScalePreference,
    ) -> Self {
        let resolved = ScaleContext::resolve(dpi, work_area, preferred_logical_size, preference);
        Self {
            dpi: resolved.dpi,
            os_scale: resolved.os_scale,
            preference: resolved.preference,
            scale: resolved.scale,
            work_area: resolved.work_area,
            preferred_logical_size: resolved.preferred_logical_size,
            physical_window_size: resolved.physical_window_size,
            compact: resolved.compact,
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn logical_point(self, physical: PhysicalPoint) -> crate::core::Point {
        self.scale.logical_point(physical)
    }

    pub fn same_monitor_metrics(self, other: Self) -> bool {
        self.dpi == other.dpi
            && self.work_area == other.work_area
            && self.preference == other.preference
    }

    pub fn clamp_origin(self, proposed: PhysicalPoint, size: PhysicalSize) -> PhysicalPoint {
        let resolved = ScaleContext {
            dpi: self.dpi,
            os_scale: self.os_scale,
            preference: self.preference,
            scale: self.scale,
            work_area: self.work_area,
            preferred_logical_size: self.preferred_logical_size,
            physical_window_size: self.physical_window_size,
            compact: self.compact,
        };
        resolved.clamp_origin(proposed, size)
    }
}

pub fn work_area_for_window(hwnd: HWND) -> WorkArea {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    work_area_for_monitor(monitor)
}

pub fn work_area_for_point(point: PhysicalPoint) -> WorkArea {
    work_area_for_monitor(monitor_for_point(point))
}

fn monitor_for_point(point: PhysicalPoint) -> HMONITOR {
    unsafe {
        MonitorFromPoint(
            POINT {
                x: point.x,
                y: point.y,
            },
            MONITOR_DEFAULTTONEAREST,
        )
    }
}

fn dpi_for_monitor(monitor: HMONITOR) -> u32 {
    let mut dpi_x = BASE_DPI;
    let mut dpi_y = BASE_DPI;
    let result = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    if result.is_ok() {
        dpi_x.max(dpi_y).max(BASE_DPI)
    } else {
        BASE_DPI
    }
}

fn work_area_for_monitor(monitor: HMONITOR) -> WorkArea {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool();
    let rect = if ok { info.rcWork } else { RECT::default() };
    let rect = if rect.right > rect.left && rect.bottom > rect.top {
        PhysicalRect::new(rect.left, rect.top, rect.right, rect.bottom)
    } else {
        PhysicalRect::new(0, 0, 1920, 1080)
    };
    WorkArea { rect }
}
