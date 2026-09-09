use crate::core::{PhysicalRect, Scene, ScenePrimitive, UiRect, UiScale};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderErrorStage {
    Create,
    Prepare,
    Draw,
    Copy,
    Present,
    Commit,
}

impl RenderErrorStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Prepare => "prepare",
            Self::Draw => "draw",
            Self::Copy => "copy",
            Self::Present => "present",
            Self::Commit => "commit",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameReason {
    #[default]
    SceneChange,
    PlatformExposure,
    Resize,
    Recovery,
    Explicit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RendererCapabilities {
    pub partial_redraw: bool,
    pub retained_surface: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameInfo<'a> {
    viewport: PhysicalRect,
    damage: &'a [PhysicalRect],
    scale: UiScale,
    reason: FrameReason,
    full_redraw: bool,
}

impl<'a> FrameInfo<'a> {
    pub fn new(
        viewport: PhysicalRect,
        damage: &'a [PhysicalRect],
        scale: UiScale,
        reason: FrameReason,
        full_redraw: bool,
    ) -> Self {
        Self {
            viewport,
            damage,
            scale,
            reason,
            full_redraw,
        }
    }

    pub fn viewport(&self) -> PhysicalRect {
        self.viewport
    }

    pub fn damage(&self) -> &'a [PhysicalRect] {
        self.damage
    }

    pub fn scale(&self) -> UiScale {
        self.scale
    }

    pub fn reason(&self) -> FrameReason {
        self.reason
    }

    pub fn is_full_redraw(&self) -> bool {
        self.full_redraw
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub painted_rects: usize,
    pub painted_pixels: u64,
}

impl RenderStats {
    pub fn for_frame(frame: &FrameInfo<'_>) -> Self {
        let rects = if frame.is_full_redraw() {
            std::slice::from_ref(&frame.viewport)
        } else {
            frame.damage
        };
        Self {
            painted_rects: rects.len(),
            painted_pixels: rects.iter().fold(0_u64, |total, rect| {
                total.saturating_add(
                    (rect.width().max(0) as u64).saturating_mul(rect.height().max(0) as u64),
                )
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryPressure {
    Moderate,
    Critical,
}

pub trait SceneRenderer: 'static {
    type Target;
    type Error;

    fn capabilities(&self) -> RendererCapabilities;

    fn prepare(
        &mut self,
        _target: &mut Self::Target,
        _frame: &FrameInfo<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn render(
        &mut self,
        target: &mut Self::Target,
        scene: &Scene,
        frame: &FrameInfo<'_>,
    ) -> Result<RenderStats, Self::Error>;

    fn trim(&mut self, _pressure: MemoryPressure) {}

    fn memory_usage(&self) -> crate::memory::CacheUsage {
        crate::memory::CacheUsage::default()
    }

    fn set_memory_budget(&mut self, _budget_bytes: usize) {}

    fn trim_to(&mut self, target_bytes: usize) -> usize {
        let before = self.memory_usage().resident_bytes();
        self.trim(if target_bytes == 0 {
            MemoryPressure::Critical
        } else {
            MemoryPressure::Moderate
        });
        before.saturating_sub(self.memory_usage().resident_bytes())
    }

    fn reset(&mut self) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRegion {
    rect: UiRect,
}

impl ClipRegion {
    pub fn new(rect: UiRect) -> Self {
        Self { rect }
    }

    pub fn rect(self) -> UiRect {
        self.rect
    }

    pub fn intersects(self, command: &ScenePrimitive) -> bool {
        command.paint_bounds().intersect(self.rect).is_some()
    }

    pub fn intersection(self, rect: UiRect) -> Option<UiRect> {
        rect.intersect(self.rect)
    }
}

#[cfg(test)]
#[path = "contract_test.rs"]
mod tests;
