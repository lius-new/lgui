use std::collections::HashSet;

use super::{HostTree, UiEvent, UiId, UiRect};

#[derive(Clone, Debug, Default)]
pub struct DirtySet {
    ids: HashSet<UiId>,
    rects: Vec<UiRect>,
}

impl DirtySet {
    pub fn new() -> Self {
        Self {
            ids: HashSet::new(),
            rects: Vec::new(),
        }
    }

    pub fn mark_id(&mut self, id: UiId) {
        self.ids.insert(id);
    }

    pub fn mark_rect(&mut self, rect: UiRect) {
        self.rects.push(rect);
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty() && self.rects.is_empty()
    }

    pub fn bounds(&self, tree: &HostTree) -> Option<UiRect> {
        let mut bounds = tree.paint_bounds(self.ids.iter().cloned());
        for rect in &self.rects {
            bounds = Some(bounds.map_or(*rect, |current| current.union(*rect)));
        }
        bounds
    }
}

#[derive(Default)]
pub struct DirtyTracker {
    dirty: DirtySet,
}

impl DirtyTracker {
    pub fn mark_id(&mut self, id: UiId) {
        self.dirty.mark_id(id);
    }

    pub fn mark_rect(&mut self, rect: UiRect) {
        self.dirty.mark_rect(rect);
    }

    pub fn mark_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::HoverChanged { previous, current } => {
                if let Some(id) = previous {
                    self.mark_id(id);
                }
                if let Some(hit) = current {
                    self.mark_id(hit.id);
                }
            }
            UiEvent::PressedChanged { previous, current } => {
                if let Some(id) = previous {
                    self.mark_id(id);
                }
                if let Some(hit) = current {
                    self.mark_id(hit.id);
                }
            }
            UiEvent::Clicked(hit) => self.mark_id(hit.id),
            UiEvent::Wheel { hit, .. } => self.mark_id(hit.id),
            UiEvent::TextInput { target, .. }
            | UiEvent::ImeStarted { target }
            | UiEvent::ImeUpdated { target, .. }
            | UiEvent::ImeEnded { target }
            | UiEvent::Backspace { target }
            | UiEvent::KeyDown { target, .. } => self.mark_id(target),
            UiEvent::PointerPressed { hit, .. } => self.mark_id(hit.id),
            UiEvent::PointerMoved { hit, .. } => self.mark_id(hit.id),
            UiEvent::PointerDragged { hit, .. } => self.mark_id(hit.id),
            UiEvent::PointerReleased { hit, .. } => self.mark_id(hit.id),
            UiEvent::FocusChanged { previous, current } => {
                if let Some(id) = previous {
                    self.mark_id(id);
                }
                if let Some(hit) = current {
                    self.mark_id(hit.id);
                }
            }
            UiEvent::PointerLeft { previous } => {
                if let Some(id) = previous {
                    self.mark_id(id);
                }
            }
        }
    }

    pub fn mark_animation_ids(&mut self, ids: impl IntoIterator<Item = UiId>) {
        for id in ids {
            self.mark_id(id);
        }
    }

    pub fn take(&mut self) -> DirtySet {
        std::mem::take(&mut self.dirty)
    }
}
