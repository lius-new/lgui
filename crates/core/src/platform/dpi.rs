use crate::core::{PhysicalPoint, PhysicalRect, PhysicalSize, Size, UiScale};

pub const BASE_DPI: u32 = 96;
pub const MIN_READABLE_SCALE: f32 = 0.80;
const WORK_AREA_MARGIN_LOGICAL: f32 = 12.0;
const AUTO_TARGET_WIDTH_RATIO: f32 = 0.72;
const AUTO_TARGET_HEIGHT_RATIO: f32 = 0.75;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ScalePreference {
    #[default]
    Auto,
    Multiplier(f32),
}

impl ScalePreference {
    pub fn multiplier(self) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Multiplier(value) => Some(value.max(0.1)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkArea {
    pub rect: PhysicalRect,
}

impl WorkArea {
    pub fn size(self) -> PhysicalSize {
        PhysicalSize::new(self.rect.width().max(1), self.rect.height().max(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleContext {
    pub dpi: u32,
    pub os_scale: f32,
    pub preference: ScalePreference,
    pub scale: UiScale,
    pub work_area: WorkArea,
    pub preferred_logical_size: Size,
    pub physical_window_size: PhysicalSize,
    pub compact: bool,
}

impl ScaleContext {
    pub fn resolve(
        dpi: u32,
        work_area: WorkArea,
        preferred_logical_size: Size,
        preference: ScalePreference,
    ) -> Self {
        let dpi = dpi.max(BASE_DPI);
        let os_scale = dpi as f32 / BASE_DPI as f32;
        let margin = (WORK_AREA_MARGIN_LOGICAL * os_scale).round() as i32;
        let available = PhysicalSize::new(
            (work_area.size().width - margin * 2).max(1),
            (work_area.size().height - margin * 2).max(1),
        );
        let fit_scale = (available.width as f32 / preferred_logical_size.width.max(1.0))
            .min(available.height as f32 / preferred_logical_size.height.max(1.0));
        let requested_scale = match preference.multiplier() {
            Some(multiplier) => os_scale * multiplier,
            None => {
                let work_size = work_area.size();
                let auto_scale = (work_size.width as f32 * AUTO_TARGET_WIDTH_RATIO
                    / preferred_logical_size.width.max(1.0))
                .min(
                    work_size.height as f32 * AUTO_TARGET_HEIGHT_RATIO
                        / preferred_logical_size.height.max(1.0),
                );
                auto_scale.clamp(MIN_READABLE_SCALE, os_scale)
            }
        };
        let effective = requested_scale.min(fit_scale).max(0.1);
        let scale = UiScale::new(effective);
        Self {
            dpi,
            os_scale,
            preference,
            scale,
            work_area,
            preferred_logical_size,
            physical_window_size: scale.physical_size(preferred_logical_size),
            compact: effective < MIN_READABLE_SCALE,
        }
    }

    pub fn same_monitor_metrics(self, other: Self) -> bool {
        self.dpi == other.dpi
            && self.work_area == other.work_area
            && self.preference == other.preference
    }

    pub fn clamp_origin(self, proposed: PhysicalPoint, size: PhysicalSize) -> PhysicalPoint {
        let max_x = (self.work_area.rect.right - size.width).max(self.work_area.rect.left);
        let max_y = (self.work_area.rect.bottom - size.height).max(self.work_area.rect.top);
        PhysicalPoint::new(
            proposed.x.clamp(self.work_area.rect.left, max_x),
            proposed.y.clamp(self.work_area.rect.top, max_y),
        )
    }
}

#[cfg(test)]
#[path = "dpi_test.rs"]
mod tests;
