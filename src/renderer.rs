use crate::{
    core::{InputEvent, RuntimeOutput, Scene, UiId, UiRect, UiScale},
    frame::InvalidationSet,
    session::UiSession,
};

pub trait RenderBackend<Target> {
    fn draw_scene(&mut self, target: Target, scene: &Scene, clip: Option<UiRect>);
}

#[derive(Clone, Debug)]
pub enum PresentRequest {
    Full,
    Dirty(Vec<UiRect>),
}

impl PresentRequest {
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    pub fn project_to_physical(self, scale: UiScale) -> Self {
        match self {
            Self::Full => Self::Full,
            Self::Dirty(rects) => Self::Dirty(
                rects
                    .into_iter()
                    .map(|rect| scale.physical_rect_outward(rect))
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentMode {
    Full,
    Dirty,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct PresentStats {
    pub mode: PresentMode,
    pub dirty_rect_count: usize,
    pub submit_scope: &'static str,
    pub fallback_reason: Option<&'static str>,
}

impl PresentStats {
    pub fn full(reason: Option<&'static str>) -> Self {
        Self::full_with_scope(reason, "full-window")
    }

    pub fn full_with_scope(reason: Option<&'static str>, submit_scope: &'static str) -> Self {
        Self {
            mode: PresentMode::Full,
            dirty_rect_count: 1,
            submit_scope,
            fallback_reason: reason,
        }
    }

    pub fn dirty(count: usize, submit_scope: &'static str) -> Self {
        Self {
            mode: PresentMode::Dirty,
            dirty_rect_count: count,
            submit_scope,
            fallback_reason: None,
        }
    }

    pub fn dirty_with_fallback(
        count: usize,
        submit_scope: &'static str,
        reason: &'static str,
    ) -> Self {
        Self {
            mode: PresentMode::Dirty,
            dirty_rect_count: count,
            submit_scope,
            fallback_reason: Some(reason),
        }
    }

    pub fn skipped(reason: &'static str) -> Self {
        Self {
            mode: PresentMode::Skipped,
            dirty_rect_count: 0,
            submit_scope: "none",
            fallback_reason: Some(reason),
        }
    }
}

pub trait UiPresenter<Target, State> {
    fn present(&mut self, target: Target, state: &State, session: &mut UiSession) -> PresentStats;
    fn handle_input(&mut self, session: &mut UiSession, input: InputEvent)
        -> Option<RuntimeOutput>;
    fn advance_animations(
        &mut self,
        session: &mut UiSession,
        elapsed_ms: f32,
    ) -> Option<RuntimeOutput>;
    fn invalidate_all(&mut self, session: &mut UiSession);
    fn invalidate_rect(&mut self, session: &mut UiSession, rect: UiRect);
    fn invalidate_node(&mut self, session: &mut UiSession, id: UiId);
    fn invalidate_route(&mut self, session: &mut UiSession);
    fn invalidate_animation(&mut self, session: &mut UiSession, id: UiId, property: &'static str);
    fn release(&mut self);
}

pub trait PresenterPlugin<State>: Send {
    fn mount(&mut self, _state: &State, _host: &mut PresenterPluginHost<'_>) {}
    fn after_input(&mut self, _output: &RuntimeOutput, _host: &mut PresenterPluginHost<'_>) {}
    fn before_present(&mut self, _state: &State, _host: &mut PresenterPluginHost<'_>) {}
    fn after_present(&mut self, _state: &State, _host: &mut PresenterPluginHost<'_>) {}
    fn unmount(&mut self, _state: Option<&State>, _host: &mut PresenterPluginHost<'_>) {}
}

pub struct PresenterPluginHost<'a> {
    invalidations: &'a mut InvalidationSet,
    viewport: Option<UiRect>,
}

impl<'a> PresenterPluginHost<'a> {
    pub fn new(invalidations: &'a mut InvalidationSet, viewport: Option<UiRect>) -> Self {
        Self {
            invalidations,
            viewport,
        }
    }

    pub fn invalidate_all(&mut self) {
        self.invalidations.invalidate_all();
    }

    pub fn invalidate_rect(&mut self, rect: UiRect) {
        self.invalidations.invalidate_rect(rect);
    }

    pub fn invalidate_route(&mut self) {
        self.invalidations.invalidate_route();
    }

    pub fn viewport(&self) -> Option<UiRect> {
        self.viewport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_projection_rounds_outward() {
        let request = PresentRequest::Dirty(vec![UiRect::new(1, 1, 3, 3)])
            .project_to_physical(UiScale::new(1.25));
        let PresentRequest::Dirty(rects) = request else {
            panic!("expected dirty request");
        };
        assert_eq!(rects, vec![UiRect::new(1, 1, 4, 4)]);
    }
}
