use std::borrow::Cow;

use crate::core::UiRect;

use super::notification::{StoreNotification, Subscription};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreMutation {
    changed: bool,
}

pub enum StoreInvalidation {
    All,
    Route,
    Rect(UiRect),
}

pub struct StoreInvalidationContext<'a> {
    pub viewport: UiRect,
    pub notification: &'a StoreNotification,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreRouteId(&'static str);

impl StoreRouteId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreLifecycleEvent {
    RouteChanged {
        previous: StoreRouteId,
        next: StoreRouteId,
    },
}

impl StoreLifecycleEvent {
    pub const fn route_changed(previous: StoreRouteId, next: StoreRouteId) -> Self {
        Self::RouteChanged { previous, next }
    }
}

#[derive(Default)]
pub struct StoreInvalidationSet {
    requests: Vec<StoreInvalidation>,
}

impl StoreMutation {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn changed() -> Self {
        Self { changed: true }
    }

    pub fn extend(mut self, other: StoreMutation) -> Self {
        self.changed |= other.changed;
        self
    }

    pub fn is_empty(&self) -> bool {
        !self.changed
    }
}

impl StoreInvalidationSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&mut self) {
        self.requests.push(StoreInvalidation::All);
    }

    pub fn route(&mut self) {
        self.requests.push(StoreInvalidation::Route);
    }

    pub fn rect(&mut self, rect: UiRect) {
        self.requests.push(StoreInvalidation::Rect(rect));
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn into_requests(self) -> Vec<StoreInvalidation> {
        self.requests
    }
}

pub trait StoreUnit: Send + Sync + 'static {
    const KEY: &'static str;
    fn unit_name() -> &'static str {
        Self::KEY
    }

    fn handle_lifecycle_event(&mut self, _event: StoreLifecycleEvent) -> StoreMutation {
        StoreMutation::new()
    }

    fn advance_animations(&mut self, _elapsed_ms: f32) -> StoreMutation {
        StoreMutation::new()
    }

    fn has_running_animations(&self) -> bool {
        false
    }

    fn presenter_subscriptions() -> Vec<Subscription> {
        Vec::new()
    }

    fn collect_presenter_invalidations(
        _context: StoreInvalidationContext<'_>,
        _invalidations: &mut StoreInvalidationSet,
    ) {
    }
}

impl StoreMutation {
    pub fn into_notification(
        self,
        store_key: &'static str,
        reason: impl Into<Cow<'static, str>>,
    ) -> StoreNotification {
        StoreNotification {
            store_key,
            reason: reason.into(),
        }
    }
}
