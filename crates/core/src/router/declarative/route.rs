use std::sync::Arc;

use crate::{
    application::AppView,
    core::{Element, RenderCx},
};

use super::{
    super::{
        matcher::{
            matches::ErasedRouteHandle,
            pattern::{MatchStep, PathPattern},
        },
        Location, RouteId,
    },
    outlet,
};

pub(super) type RouteMatcher<R> =
    Arc<dyn Fn(&R, usize) -> Option<MatchStep> + Send + Sync + 'static>;

pub(super) enum RouteKind<R> {
    Match(RouteMatcher<R>),
    Layout,
    Fallback(RouteMatcher<R>),
}

pub struct Route<R> {
    pub(super) pattern: &'static str,
    pub(super) kind: RouteKind<R>,
    pub(super) view: AppView,
    pub(super) children: Vec<Route<R>>,
    pub(super) handle: Option<ErasedRouteHandle>,
    pub(super) id: RouteId,
}

impl<R> Route<R> {
    pub fn path(&self) -> &'static str {
        self.pattern
    }

    pub fn children(mut self, children: impl IntoRoutes<R>) -> Self {
        self.children = children.into_routes();
        self
    }

    /// Attaches application-owned metadata to a route. lgui stores it opaquely;
    /// layouts can retrieve it from the active `RouteMatches` chain.
    pub fn handle<T>(mut self, handle: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.handle = Some(ErasedRouteHandle::new(handle));
        self
    }
}

/// Defines a path-matched route. Root patterns are absolute and child patterns are relative.
/// `:name` captures one decoded segment and `*name` captures the remaining decoded path.
pub fn route(
    pattern: &'static str,
    view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
) -> Route<Location> {
    let pattern = PathPattern::parse(pattern);
    let source = pattern.source;
    Route {
        pattern: source,
        kind: RouteKind::Match(Arc::new(move |location, offset| {
            pattern.match_location(location, offset)
        })),
        view: Arc::new(view),
        children: Vec::new(),
        handle: None,
        id: RouteId::UNASSIGNED,
    }
}

/// Defines the default child for a parent path.
pub fn index(
    view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
) -> Route<Location> {
    Route {
        pattern: "<index>",
        kind: RouteKind::Match(Arc::new(|location: &Location, offset| {
            (offset == location.segments().len()).then(|| MatchStep {
                next_offset: offset,
                can_end: true,
                score: 5,
                pathname: None,
                params: Vec::new(),
                location: Some(location.clone()),
            })
        })),
        view: Arc::new(view),
        children: Vec::new(),
        handle: None,
        id: RouteId::UNASSIGNED,
    }
}

/// Defines a pathless visual layout. Its view should include `outlet()` where the
/// selected descendant belongs.
pub fn layout<R>(
    view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
    children: impl IntoRoutes<R>,
) -> Route<R> {
    Route {
        pattern: "<layout>",
        kind: RouteKind::Layout,
        view: Arc::new(view),
        children: children.into_routes(),
        handle: None,
        id: RouteId::UNASSIGNED,
    }
}

/// Groups child routes below a path without adding another visual component.
pub fn scope(pattern: &'static str, children: impl IntoRoutes<Location>) -> Route<Location> {
    route(pattern, |_| outlet()).children(children)
}

/// Defines a sibling fallback. Normal siblings are always attempted before it.
pub fn not_found(
    view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
) -> Route<Location> {
    Route {
        pattern: "<not-found>",
        kind: RouteKind::Fallback(Arc::new(|location: &Location, _| {
            Some(MatchStep {
                next_offset: location.segments().len(),
                can_end: true,
                score: 0,
                pathname: Some(location.path().to_owned()),
                params: Vec::new(),
                location: Some(location.clone()),
            })
        })),
        view: Arc::new(view),
        children: Vec::new(),
        handle: None,
        id: RouteId::UNASSIGNED,
    }
}

pub trait IntoRoutes<R> {
    fn into_routes(self) -> Vec<Route<R>>;
}

impl<R> IntoRoutes<R> for Route<R> {
    fn into_routes(self) -> Vec<Route<R>> {
        vec![self]
    }
}

macro_rules! tuple_routes {
    ($($name:ident),+) => {
        impl<R, $($name),+> IntoRoutes<R> for ($($name,)+)
        where
            $($name: IntoRoutes<R>,)+
        {
            fn into_routes(self) -> Vec<Route<R>> {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                let mut routes = Vec::new();
                $(routes.extend($name.into_routes());)+
                routes
            }
        }
    };
}

tuple_routes!(A, B);
tuple_routes!(A, B, C);
tuple_routes!(A, B, C, D);
tuple_routes!(A, B, C, D, E);
tuple_routes!(A, B, C, D, E, F);
tuple_routes!(A, B, C, D, E, F, G);
tuple_routes!(A, B, C, D, E, F, G, H);
tuple_routes!(A, B, C, D, E, F, G, H, I);
tuple_routes!(A, B, C, D, E, F, G, H, I, J);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
