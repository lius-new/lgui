use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use crate::core::{Observable, RenderCx};

use super::super::{location::resolve_path, Location, PathParams};

#[derive(Clone)]
pub(in crate::router) struct ErasedRouteHandle {
    pub type_id: TypeId,
    pub value: Arc<dyn Any + Send + Sync>,
}

impl ErasedRouteHandle {
    pub fn new<T>(value: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        Self {
            type_id: TypeId::of::<T>(),
            value: Arc::new(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteId(pub(in crate::router) u64);

impl RouteId {
    pub(in crate::router) const UNASSIGNED: Self = Self(0);

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone)]
pub struct RouteMatch {
    pub(in crate::router) id: RouteId,
    pub(in crate::router) pattern: &'static str,
    pub(in crate::router) pathname: String,
    pub(in crate::router) params: PathParams,
    pub(in crate::router) handle: Option<ErasedRouteHandle>,
}

impl RouteMatch {
    pub fn id(&self) -> RouteId {
        self.id
    }

    pub fn pattern(&self) -> &'static str {
        self.pattern
    }

    pub fn pathname(&self) -> &str {
        &self.pathname
    }

    pub fn params(&self) -> &PathParams {
        &self.params
    }

    pub fn handle<T>(&self) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.handle
            .as_ref()
            .filter(|handle| handle.type_id == TypeId::of::<T>())
            .and_then(|handle| handle.value.downcast_ref::<T>())
            .cloned()
    }
}

impl PartialEq for RouteMatch {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.pattern == other.pattern
            && self.pathname == other.pathname
            && self.params == other.params
            && self.handle.as_ref().map(|handle| handle.type_id)
                == other.handle.as_ref().map(|handle| handle.type_id)
    }
}

impl Eq for RouteMatch {}

#[derive(Clone, PartialEq, Eq)]
pub struct RouteMatches {
    pub(in crate::router) location: Option<Location>,
    pub(in crate::router) entries: Vec<RouteMatch>,
}

impl RouteMatches {
    pub fn location(&self) -> Option<&Location> {
        self.location.as_ref()
    }

    pub fn entries(&self) -> &[RouteMatch] {
        &self.entries
    }

    pub fn current(&self) -> Option<&RouteMatch> {
        self.entries.last()
    }

    /// Returns the nearest metadata value, allowing child routes to override and
    /// otherwise inherit application navigation configuration from their parent.
    pub fn deepest_handle<T>(&self) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.entries.iter().rev().find_map(RouteMatch::handle::<T>)
    }

    /// Resolves using route-tree ancestry. Each leading `..` selects the previous
    /// matched route, even when the current route consumed multiple URL segments.
    pub fn resolve_from(&self, depth: usize, target: impl AsRef<str>) -> Option<Location> {
        let target = target.as_ref();
        if target.starts_with('/') {
            return Some(Location::new(target));
        }
        let mut depth = depth.min(self.entries.len().checked_sub(1)?);
        let mut remainder = target;
        while remainder == ".." || remainder.starts_with("../") {
            depth = depth.saturating_sub(1);
            remainder = remainder.strip_prefix("../").unwrap_or("");
            if remainder.is_empty() {
                break;
            }
        }
        let base = self.entries.get(depth)?.pathname();
        Some(Location::new(resolve_path(base, remainder)))
    }

    pub fn resolve(&self, target: impl AsRef<str>) -> Option<Location> {
        self.resolve_from(self.entries.len().checked_sub(1)?, target)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::router) struct CurrentRouteMatch(pub RouteMatch);

#[derive(Clone)]
pub(in crate::router) struct RouteMatchesObservable(pub Observable<RouteMatches>);

impl PartialEq for RouteMatchesObservable {
    fn eq(&self, other: &Self) -> bool {
        self.0.id() == other.0.id()
    }
}

pub trait RouteMatchHooks {
    fn use_route_matches(&mut self) -> RouteMatches;
    fn use_route_match(&mut self) -> RouteMatch;
    fn use_path_params(&mut self) -> PathParams;
}

impl RouteMatchHooks for RenderCx<'_, '_> {
    fn use_route_matches(&mut self) -> RouteMatches {
        let source = self.use_context::<RouteMatchesObservable>().0;
        self.use_observable(source, clone_route_matches)
    }

    fn use_route_match(&mut self) -> RouteMatch {
        self.use_context::<CurrentRouteMatch>().0
    }

    fn use_path_params(&mut self) -> PathParams {
        self.use_route_match().params
    }
}

fn clone_route_matches(matches: &RouteMatches) -> RouteMatches {
    matches.clone()
}
