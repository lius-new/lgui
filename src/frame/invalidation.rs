use crate::core::{UiId, UiRect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidationRequest {
    All,
    Rect(UiRect),
    Node(UiId),
    Route,
    Animation { id: UiId, property: &'static str },
}

#[derive(Clone, Debug, Default)]
pub struct InvalidationSet {
    requests: Vec<InvalidationRequest>,
}

impl InvalidationSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate_all(&mut self) {
        self.requests.push(InvalidationRequest::All);
    }

    pub fn invalidate_rect(&mut self, rect: UiRect) {
        self.requests.push(InvalidationRequest::Rect(rect));
    }

    pub fn invalidate_node(&mut self, id: UiId) {
        self.requests.push(InvalidationRequest::Node(id));
    }

    pub fn invalidate_route(&mut self) {
        self.requests.push(InvalidationRequest::Route);
    }

    pub fn invalidate_animation(&mut self, id: UiId, property: &'static str) {
        self.requests
            .push(InvalidationRequest::Animation { id, property });
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = InvalidationRequest> + '_ {
        self.requests.drain(..)
    }
}
