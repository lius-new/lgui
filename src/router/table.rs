use std::{collections::HashSet, sync::Arc};

use crate::{
    application::AppView,
    core::{
        component, context_provider, group, Element, Observable, ObservableListener, RenderCx,
        UiEffect,
    },
};

use super::{
    matches::{CurrentRouteMatch, ErasedRouteHandle, RouteMatchesObservable},
    pattern::{MatchStep, PathPattern},
    Location, PathParams, RouteId, RouteMatch, RouteMatches, Router,
};

type RouteMatcher<R> = Arc<dyn Fn(&R, usize) -> Option<MatchStep> + Send + Sync + 'static>;

enum RouteKind<R> {
    Match(RouteMatcher<R>),
    Layout,
    Fallback(RouteMatcher<R>),
}

pub struct Route<R> {
    pattern: &'static str,
    kind: RouteKind<R>,
    view: AppView,
    children: Vec<Route<R>>,
    handle: Option<ErasedRouteHandle>,
    id: RouteId,
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

/// Defines an absolute replace redirect. Redirects run after a committed render.
pub fn redirect(pattern: &'static str, destination: &'static str) -> Route<Location> {
    assert!(
        destination.starts_with('/'),
        "redirect destinations must be absolute"
    );
    let destination = Location::new(destination);
    route(pattern, move |cx| {
        let current = cx.use_route::<Location>();
        let replace = cx.use_replace::<Location>();
        let destination = destination.clone();
        let effect_destination = destination.clone();
        cx.use_effect((current.clone(), destination), move || {
            if current != effect_destination {
                replace(effect_destination);
            }
        });
        group(cx.viewport())
    })
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

pub struct DeclarativeRouter<R> {
    routes: Arc<Vec<Route<R>>>,
}

impl<R> Clone for DeclarativeRouter<R> {
    fn clone(&self) -> Self {
        Self {
            routes: Arc::clone(&self.routes),
        }
    }
}

pub fn create_router<R>(routes: impl IntoRoutes<R>) -> DeclarativeRouter<R>
where
    R: Clone + PartialEq,
{
    let mut routes = routes.into_routes();
    assert!(
        !routes.is_empty(),
        "a router must define at least one route"
    );
    validate_routes(&routes);
    assign_route_ids(&mut routes, &mut 1);
    DeclarativeRouter {
        routes: Arc::new(routes),
    }
}

impl<R> DeclarativeRouter<R>
where
    R: Default + Clone + PartialEq + Send + Sync + 'static,
{
    pub fn matches(&self, target: &R) -> Option<RouteMatches> {
        self.resolve(target).map(|branch| branch.matches.clone())
    }

    pub fn outlet(&self, cx: &mut RenderCx<'_, '_>) -> Element {
        let router = cx.application().router::<R>();
        let resolver: Arc<dyn OutletResolver> = Arc::new(TypedOutletResolver {
            routes: self.clone(),
            router,
        });
        context_provider(
            route_matches_observable(Arc::clone(&resolver)),
            context_provider(OutletContext { resolver, depth: 0 }, outlet()),
        )
    }

    fn resolve(&self, target: &R) -> Option<ResolvedBranch> {
        let params = PathParams::default();
        let nodes = resolve_routes(&self.routes, target, 0, "/", &params)?.nodes;
        let location = nodes.iter().find_map(|node| node.location.clone());
        let matches = RouteMatches {
            location,
            entries: nodes.iter().map(|node| node.matched.clone()).collect(),
        };
        Some(ResolvedBranch { nodes, matches })
    }
}

/// An Outlet is a retained component boundary. Parent layouts do not subscribe to
/// child selection and therefore keep local state/effects while descendants change.
pub fn outlet() -> Element {
    component((), |cx, _| cx.use_context::<OutletContext>().render(cx)).key("router.outlet")
}

type OutletView = Arc<
    dyn for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
>;

#[derive(Clone, PartialEq, Eq)]
struct OutletIdentity {
    matched: Option<RouteMatch>,
    leaf_location: Option<Location>,
}

#[derive(Clone)]
struct OutletSelection {
    identity: OutletIdentity,
    view: OutletView,
}

impl PartialEq for OutletSelection {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

trait OutletResolver: Send + Sync {
    fn id(&self) -> u64;
    fn matches(&self) -> RouteMatches;
    fn select(self: Arc<Self>, depth: usize) -> OutletSelection;
    fn subscribe(&self, listener: ObservableListener) -> UiEffect;
}

struct TypedOutletResolver<R> {
    routes: DeclarativeRouter<R>,
    router: Router<R>,
}

impl<R> OutletResolver for TypedOutletResolver<R>
where
    R: Default + Clone + PartialEq + Send + Sync + 'static,
{
    fn id(&self) -> u64 {
        let table = Arc::as_ptr(&self.routes.routes) as usize as u64;
        self.router
            .observable_id()
            .wrapping_mul(0x517C_C1B7_2722_0A95)
            ^ table.rotate_left(17)
    }

    fn matches(&self) -> RouteMatches {
        self.routes
            .resolve(&self.router.current())
            .unwrap_or_else(|| panic!("current location has no declarative component"))
            .matches
    }

    fn select(self: Arc<Self>, depth: usize) -> OutletSelection {
        let snapshot = self.router.snapshot();
        let branch = Arc::new(
            self.routes
                .resolve(snapshot.current())
                .unwrap_or_else(|| panic!("current location has no declarative component")),
        );
        let matched = branch.nodes.get(depth).map(|node| node.matched.clone());
        let leaf_location = (depth + 1 == branch.nodes.len())
            .then(|| branch.matches.location.clone())
            .flatten();
        let identity = OutletIdentity {
            matched,
            leaf_location,
        };
        let router_context = self.router.context_from(snapshot);
        let resolver: Arc<dyn OutletResolver> = self;
        let view = if depth >= branch.nodes.len() {
            Arc::new(|cx: &mut RenderCx<'_, '_>| group(cx.viewport())) as OutletView
        } else {
            Arc::new(move |_cx: &mut RenderCx<'_, '_>| {
                context_provider(
                    router_context.clone(),
                    context_provider(
                        branch.matches.clone(),
                        render_branch(Arc::clone(&branch), depth, Arc::clone(&resolver)),
                    ),
                )
            })
        };
        OutletSelection { identity, view }
    }

    fn subscribe(&self, listener: ObservableListener) -> UiEffect {
        let cleanup_router = self.router.clone();
        let token = self.router.subscribe(move |_| listener());
        Box::new(move || {
            cleanup_router.unsubscribe(token);
        })
    }
}

struct ResolvedNode {
    matched: RouteMatch,
    view: AppView,
    location: Option<Location>,
}

struct ResolvedBranch {
    nodes: Vec<ResolvedNode>,
    matches: RouteMatches,
}

struct ResolvedCandidate {
    nodes: Vec<ResolvedNode>,
    score: u32,
}

#[derive(Clone)]
struct OutletContext {
    resolver: Arc<dyn OutletResolver>,
    depth: usize,
}

impl OutletContext {
    fn render(&self, cx: &mut RenderCx<'_, '_>) -> Element {
        let read_resolver = Arc::clone(&self.resolver);
        let subscribe_resolver = Arc::clone(&self.resolver);
        let depth = self.depth;
        let observable = Observable::new(
            outlet_observable_id(self.resolver.id(), depth),
            move || Arc::clone(&read_resolver).select(depth),
            move |listener| subscribe_resolver.subscribe(listener),
        );
        let selection = cx.use_observable(observable, clone_outlet_selection);
        (selection.view)(cx)
    }
}

impl PartialEq for OutletContext {
    fn eq(&self, other: &Self) -> bool {
        self.depth == other.depth && self.resolver.id() == other.resolver.id()
    }
}

fn clone_outlet_selection(selection: &OutletSelection) -> OutletSelection {
    selection.clone()
}

fn route_matches_observable(resolver: Arc<dyn OutletResolver>) -> RouteMatchesObservable {
    let read_resolver = Arc::clone(&resolver);
    let subscribe_resolver = Arc::clone(&resolver);
    RouteMatchesObservable(Observable::new(
        resolver.id().wrapping_add(0xA11C_E5A7_0000_0000),
        move || read_resolver.matches(),
        move |listener| subscribe_resolver.subscribe(listener),
    ))
}

fn outlet_observable_id(router_id: u64, depth: usize) -> u64 {
    router_id
        .wrapping_mul(0x9E37_79B1_85EB_CA87)
        .wrapping_add(depth as u64)
}

fn render_branch(
    branch: Arc<ResolvedBranch>,
    depth: usize,
    resolver: Arc<dyn OutletResolver>,
) -> Element {
    let node = &branch.nodes[depth];
    let matched = node.matched.clone();
    let view = Arc::clone(&node.view);
    let key = format!("router.route.{}", matched.id().get());
    let child_outlet = OutletContext {
        resolver,
        depth: depth + 1,
    };
    context_provider(
        CurrentRouteMatch(matched),
        context_provider(child_outlet, component((), move |cx, _| view(cx)).key(key)),
    )
}

fn resolve_routes<R>(
    routes: &[Route<R>],
    target: &R,
    offset: usize,
    parent_pathname: &str,
    parent_params: &PathParams,
) -> Option<ResolvedCandidate>
where
    R: PartialEq,
{
    resolve_route_pass(
        routes,
        target,
        offset,
        parent_pathname,
        parent_params,
        false,
    )
    .or_else(|| resolve_route_pass(routes, target, offset, parent_pathname, parent_params, true))
}

fn resolve_route_pass<R>(
    routes: &[Route<R>],
    target: &R,
    offset: usize,
    parent_pathname: &str,
    parent_params: &PathParams,
    fallback: bool,
) -> Option<ResolvedCandidate>
where
    R: PartialEq,
{
    let mut best: Option<ResolvedCandidate> = None;
    for route in routes {
        if matches!(route.kind, RouteKind::Fallback(_)) != fallback {
            continue;
        }
        let Some(step) = route.try_match(target, offset, parent_pathname) else {
            continue;
        };
        let mut params = parent_params.clone();
        if !params.extend(step.params.clone()) {
            continue;
        }
        let pathname = step
            .pathname
            .clone()
            .unwrap_or_else(|| parent_pathname.to_owned());
        let matched = RouteMatch {
            id: route.id,
            pattern: route.pattern,
            pathname: pathname.clone(),
            params: params.clone(),
            handle: route.handle.clone(),
        };
        let node = ResolvedNode {
            matched,
            view: Arc::clone(&route.view),
            location: step.location,
        };
        let candidate = if let Some(mut children) = resolve_routes(
            &route.children,
            target,
            step.next_offset,
            &pathname,
            &params,
        ) {
            let mut nodes = Vec::with_capacity(children.nodes.len() + 1);
            nodes.push(node);
            nodes.append(&mut children.nodes);
            Some(ResolvedCandidate {
                nodes,
                score: step.score + children.score,
            })
        } else if step.can_end {
            Some(ResolvedCandidate {
                nodes: vec![node],
                score: step.score,
            })
        } else {
            None
        };
        if let Some(candidate) = candidate {
            if best
                .as_ref()
                .map_or(true, |current| candidate.score > current.score)
            {
                best = Some(candidate);
            }
        }
    }
    best
}

impl<R: PartialEq> Route<R> {
    fn try_match(&self, target: &R, offset: usize, parent_pathname: &str) -> Option<MatchStep> {
        match &self.kind {
            RouteKind::Match(matcher) | RouteKind::Fallback(matcher) => matcher(target, offset),
            RouteKind::Layout => Some(MatchStep {
                next_offset: offset,
                can_end: false,
                score: 0,
                pathname: Some(parent_pathname.to_owned()),
                params: Vec::new(),
                location: None,
            }),
        }
    }
}

fn validate_routes<R>(routes: &[Route<R>])
where
    R: Clone + PartialEq,
{
    validate_level(routes, false);
    let mut terminal_patterns = HashSet::new();
    validate_terminal_patterns(routes, "/", &mut terminal_patterns);
}

fn validate_terminal_patterns<R>(
    routes: &[Route<R>],
    parent: &str,
    terminal_patterns: &mut HashSet<String>,
) {
    for route in routes {
        match &route.kind {
            RouteKind::Layout => {
                validate_terminal_patterns(&route.children, parent, terminal_patterns);
            }
            RouteKind::Match(_) if route.pattern == "<index>" => {
                insert_terminal_pattern(parent, terminal_patterns);
            }
            RouteKind::Match(_) => {
                let full = join_route_pattern(parent, route.pattern);
                validate_full_parameter_names(&full);
                let has_index = route
                    .children
                    .iter()
                    .any(|child| child.pattern == "<index>");
                if !has_index {
                    insert_terminal_pattern(&full, terminal_patterns);
                }
                validate_terminal_patterns(&route.children, &full, terminal_patterns);
            }
            RouteKind::Fallback(_) => {
                insert_terminal_pattern(&format!("<not-found>{parent}"), terminal_patterns);
            }
        }
    }
}

fn validate_full_parameter_names(pattern: &str) {
    let mut names = HashSet::new();
    for segment in pattern.split('/') {
        let name = segment
            .strip_prefix(':')
            .or_else(|| segment.strip_prefix('*'));
        if let Some(name) = name {
            assert!(
                names.insert(name),
                "nested route parameter `{name}` shadows an ancestor parameter"
            );
        }
    }
}

fn insert_terminal_pattern(pattern: &str, terminal_patterns: &mut HashSet<String>) {
    let shape = pattern
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment.starts_with(':') {
                ":"
            } else if segment.starts_with('*') {
                "*"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let shape = format!("/{shape}");
    assert!(
        terminal_patterns.insert(shape.clone()),
        "ambiguous complete route pattern `{shape}`"
    );
}

fn join_route_pattern(parent: &str, pattern: &str) -> String {
    if pattern.starts_with('/') {
        return pattern.to_owned();
    }
    if parent == "/" {
        format!("/{pattern}")
    } else {
        format!("{}/{pattern}", parent.trim_end_matches('/'))
    }
}

fn validate_level<R>(routes: &[Route<R>], inside_path: bool) {
    let mut sibling_patterns = HashSet::new();
    let mut saw_fallback = false;
    for route in routes {
        match &route.kind {
            RouteKind::Layout => {
                assert!(!route.children.is_empty(), "route layouts require children");
            }
            RouteKind::Fallback(_) => {
                assert!(
                    !saw_fallback,
                    "only one not-found route is allowed per level"
                );
                saw_fallback = true;
                assert!(
                    route.children.is_empty(),
                    "not-found routes cannot have children"
                );
            }
            RouteKind::Match(_) => {
                if route.pattern == "<index>" {
                    assert!(inside_path, "index routes require a parent path");
                    assert!(
                        route.children.is_empty(),
                        "index routes cannot have children"
                    );
                } else {
                    let absolute = route.pattern.starts_with('/');
                    let message = if inside_path {
                        "child route patterns must be relative"
                    } else {
                        "root route patterns must be absolute"
                    };
                    assert!(absolute != inside_path, "{message}");
                }
            }
        }
        assert!(
            !saw_fallback || matches!(route.kind, RouteKind::Fallback(_)),
            "the not-found route must be the final sibling"
        );
        if !matches!(route.kind, RouteKind::Layout) {
            assert!(
                sibling_patterns.insert(route.pattern),
                "duplicate sibling route pattern `{}`",
                route.pattern
            );
        }
        let child_inside_path =
            inside_path || matches!(route.kind, RouteKind::Match(_)) && route.pattern != "<index>";
        validate_level(&route.children, child_inside_path);
    }
}

fn assign_route_ids<R>(routes: &mut [Route<R>], next: &mut u64) {
    for route in routes {
        route.id = RouteId(*next);
        *next += 1;
        assign_route_ids(&mut route.children, next);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::{
        application::{AppView, ApplicationContext},
        core::{
            component, context_provider, text, Color, HostTree, HostTreeBuilder, InputEvent,
            InteractionRole, Point, PointerButton, PointerData, RootComponent, TextStyle,
            UiElement, UiRect, UiRuntime, UiScale, VisualStyle,
        },
        router::RouteMatchHooks as _,
        session::UiSession,
    };

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum NavigationHandle {
        Primary,
        Section(&'static str),
        Detail,
    }

    fn empty(cx: &mut RenderCx<'_, '_>) -> Element {
        group(cx.viewport())
    }

    #[test]
    fn nested_routes_match_layouts_index_children_and_dynamic_parameters() {
        let router = create_router(layout(
            |_| outlet(),
            scope(
                "/community",
                (
                    index(empty).handle(NavigationHandle::Section("feed")),
                    route("events", empty).handle(NavigationHandle::Section("events")),
                    route("articles/:article_id", empty).handle(NavigationHandle::Detail),
                ),
            )
            .handle(NavigationHandle::Primary),
        ));

        let matches = router
            .matches(&Location::new(
                "/community/articles/hello%20world?tab=comments",
            ))
            .expect("nested route match");
        assert_eq!(matches.entries().len(), 3);
        assert_eq!(matches.location().unwrap().query(), Some("tab=comments"));
        assert_eq!(
            matches.current().unwrap().params().get("article_id"),
            Some("hello world")
        );
        assert_eq!(
            matches.deepest_handle::<NavigationHandle>(),
            Some(NavigationHandle::Detail)
        );
        assert_eq!(matches.resolve("..").unwrap().path(), "/community");
    }

    #[test]
    fn wildcard_parameters_capture_and_decode_the_remaining_path() {
        let router = create_router(route("/files/*path", empty));
        let matches = router
            .matches(&Location::new("/files/images/hello%20world.png"))
            .expect("wildcard match");

        assert_eq!(
            matches.current().unwrap().params().get("path"),
            Some("images/hello world.png")
        );
    }

    #[test]
    fn child_routes_inherit_nearest_parent_handle() {
        let router = create_router(
            scope("/community", route("events", empty))
                .handle(NavigationHandle::Section("community")),
        );
        let matches = router
            .matches(&Location::new("/community/events"))
            .expect("route match");
        assert_eq!(
            matches.deepest_handle::<NavigationHandle>(),
            Some(NavigationHandle::Section("community"))
        );
    }

    #[test]
    fn fallback_runs_only_after_normal_siblings() {
        let router = create_router((
            route("/store", empty),
            not_found(empty).handle(NavigationHandle::Detail),
        ));
        assert_eq!(
            router
                .matches(&Location::new("/store"))
                .unwrap()
                .current()
                .unwrap()
                .pattern(),
            "/store"
        );
        assert_eq!(
            router
                .matches(&Location::new("/missing"))
                .unwrap()
                .current()
                .unwrap()
                .pattern(),
            "<not-found>"
        );
    }

    #[test]
    fn matching_prefers_the_most_specific_complete_branch() {
        let router = create_router((
            route("/community/:article_id", empty),
            scope(
                "/community",
                route("events", empty).handle(NavigationHandle::Section("events")),
            ),
        ));
        let matches = router
            .matches(&Location::new("/community/events"))
            .expect("specific nested branch");

        assert_eq!(matches.entries().len(), 2);
        assert_eq!(matches.current().unwrap().pattern(), "events");
        assert_eq!(
            matches.deepest_handle::<NavigationHandle>(),
            Some(NavigationHandle::Section("events"))
        );
    }

    #[test]
    fn invalid_tree_shapes_fail_during_router_creation() {
        assert!(std::panic::catch_unwind(|| { create_router(route("relative", empty)) }).is_err());
        assert!(std::panic::catch_unwind(|| {
            create_router(route("/root", empty).children(route("/absolute", empty)))
        })
        .is_err());
        assert!(
            std::panic::catch_unwind(|| { create_router(route("/files/*path/more", empty)) })
                .is_err()
        );
        assert!(std::panic::catch_unwind(|| {
            create_router((
                route("/community/articles/:article_id", empty),
                scope("/community", route("articles/:id", empty)),
            ))
        })
        .is_err());
        assert!(std::panic::catch_unwind(|| {
            create_router(route("/users/:id", empty).children(route("posts/:id", empty)))
        })
        .is_err());
    }

    struct NestedRouterRoot {
        application: ApplicationContext,
        routes: DeclarativeRouter<Location>,
    }

    impl RootComponent for NestedRouterRoot {
        fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> Element {
            let routes = self.routes;
            context_provider(
                self.application,
                component((), move |cx, _| routes.outlet(cx)),
            )
        }
    }

    fn mount_nested_router(
        ui: &UiRuntime,
        application: ApplicationContext,
        routes: DeclarativeRouter<Location>,
    ) -> HostTree {
        let viewport = UiRect::new(0.0, 0.0, 100.0, 100.0);
        let interaction = ui.interaction_state();
        let mut builder = HostTreeBuilder::new();
        builder.mount(
            NestedRouterRoot {
                application,
                routes,
            },
            viewport,
            &interaction,
            ui.animations(),
            ui.component_states(),
            ui.component_tree(),
            ui.contexts(),
            ui.hook_states(),
            ui.hook_updates(),
            ui.task_spawner(),
            ui.effects(),
            UiScale::ONE,
        );
        builder.finish()
    }

    #[test]
    fn nested_outlet_preserves_parent_layout_lifecycle_across_child_navigation() {
        let layout_mounts = Arc::new(AtomicUsize::new(0));
        let layout_cleanups = Arc::new(AtomicUsize::new(0));
        let feed_renders = Arc::new(AtomicUsize::new(0));
        let event_renders = Arc::new(AtomicUsize::new(0));

        let routes = create_router(layout(
            {
                let mounts = Arc::clone(&layout_mounts);
                let cleanups = Arc::clone(&layout_cleanups);
                move |cx| {
                    let mounts = Arc::clone(&mounts);
                    let cleanups = Arc::clone(&cleanups);
                    cx.use_effect((), move || {
                        mounts.fetch_add(1, Ordering::SeqCst);
                        move || {
                            cleanups.fetch_add(1, Ordering::SeqCst);
                        }
                    });
                    outlet()
                }
            },
            scope(
                "/community",
                (
                    index({
                        let renders = Arc::clone(&feed_renders);
                        move |cx| {
                            renders.fetch_add(1, Ordering::SeqCst);
                            group(cx.viewport())
                        }
                    }),
                    route("events", {
                        let renders = Arc::clone(&event_renders);
                        move |cx| {
                            renders.fetch_add(1, Ordering::SeqCst);
                            group(cx.viewport())
                        }
                    }),
                ),
            ),
        ));
        let application = ApplicationContext::empty();
        let router = application.router::<Location>();
        router.replace(Location::new("/community"));
        let mut ui = UiRuntime::new();

        mount_nested_router(&ui, application.clone(), routes.clone());
        ui.run_effects();
        assert_eq!(layout_mounts.load(Ordering::SeqCst), 1);
        assert_eq!(feed_renders.load(Ordering::SeqCst), 1);

        router.navigate(Location::new("/community/events"));
        assert!(!ui
            .apply_pending_updates(&crate::core::HostTree::new())
            .dirty_ids
            .is_empty());
        mount_nested_router(&ui, application, routes);
        ui.run_effects();

        assert_eq!(layout_mounts.load(Ordering::SeqCst), 1);
        assert_eq!(layout_cleanups.load(Ordering::SeqCst), 0);
        assert_eq!(event_renders.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn navigation_invalidates_the_outlet_bounds_instead_of_the_window() {
        let outlet_bounds = UiRect::new(18.0, 24.0, 82.0, 76.0);
        let routes = create_router((
            route("/", move |_| group(outlet_bounds)),
            route("/dialog", move |_| group(outlet_bounds)),
        ));
        let application = ApplicationContext::empty();
        let router = application.router::<Location>();
        let mut ui = UiRuntime::new();
        let old_tree = mount_nested_router(&ui, application, routes);
        ui.run_effects();

        router.navigate(Location::new("/dialog"));
        let updates = ui.apply_pending_updates(&crate::core::HostTree::new());

        assert!(!updates.dirty_ids.is_empty());
        assert_eq!(
            old_tree.paint_bounds(updates.dirty_ids),
            Some(outlet_bounds)
        );
        assert_ne!(outlet_bounds, UiRect::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn navigation_commits_the_new_nested_outlet_in_the_first_retained_frame() {
        let viewport = UiRect::new(0.0, 0.0, 320.0, 200.0);
        let outlet_bounds = UiRect::new(80.0, 40.0, 280.0, 180.0);
        let login_link_bounds = UiRect::new(180.0, 140.0, 260.0, 170.0);
        let application = ApplicationContext::empty();
        let router = application.router::<Location>();
        let view: AppView = Arc::new({
            let application = application.clone();
            move |_cx| {
                let routes = create_router(layout(
                    move |_| group(viewport).content(outlet()),
                    (
                        route("/", move |_| {
                            group(outlet_bounds).content((
                                text(
                                    outlet_bounds,
                                    "login",
                                    TextStyle::new(Color::WHITE, 16.0, 400),
                                ),
                                Element::new(move |cx| {
                                    UiElement::panel(
                                        cx.id,
                                        login_link_bounds,
                                        VisualStyle::filled(Color(0xCC3344)),
                                    )
                                    .interaction(InteractionRole::Navigation)
                                }),
                            ))
                        }),
                        route("/register", move |_| {
                            text(
                                outlet_bounds,
                                "register",
                                TextStyle::new(Color::WHITE, 16.0, 400),
                            )
                        }),
                    ),
                ));
                context_provider(
                    application.clone(),
                    component((), move |cx, _| routes.outlet(cx)).key("test.router"),
                )
            }
        });
        let mut session = UiSession::new();

        session.render_view(&view, viewport, UiScale::ONE);
        session.runtime().run_effects();
        assert!(session
            .tree()
            .nodes()
            .iter()
            .any(|node| node.text.as_deref() == Some("login")));

        session.handle_input(InputEvent::PointerDown {
            pointer: PointerData::mouse(Point::new(
                login_link_bounds.left + 1.0,
                login_link_bounds.top + 1.0,
            )),
            button: PointerButton::Left,
        });
        assert!(session.runtime().interaction_state().focused.is_some());
        router.navigate(Location::new("/register"));
        assert!(!session.apply_pending_updates().dirty_ids.is_empty());
        let commit = session.render_view(&view, viewport, UiScale::ONE);

        assert!(session
            .tree()
            .nodes()
            .iter()
            .any(|node| node.text.as_deref() == Some("register")));
        assert!(!session
            .tree()
            .nodes()
            .iter()
            .any(|node| node.text.as_deref() == Some("login")));
        assert!(commit
            .damage
            .dirty
            .effective_rects()
            .iter()
            .any(|dirty| dirty.intersect(outlet_bounds).is_some()));
    }

    #[test]
    fn route_match_consumers_refresh_alongside_the_changed_outlet() {
        let metadata_renders = Arc::new(AtomicUsize::new(0));
        let routes = create_router(layout(
            {
                let metadata_renders = Arc::clone(&metadata_renders);
                move |cx| {
                    let metadata_renders = Arc::clone(&metadata_renders);
                    group(cx.viewport()).content((
                        component((), move |cx, _| {
                            metadata_renders.fetch_add(1, Ordering::SeqCst);
                            let _matches = cx.use_route_matches();
                            group(UiRect::new(0.0, 0.0, 100.0, 20.0))
                        })
                        .key("route.metadata"),
                        outlet(),
                    ))
                }
            },
            (
                route("/", |_| group(UiRect::new(0.0, 20.0, 100.0, 100.0))),
                route("/dialog", |_| group(UiRect::new(0.0, 20.0, 100.0, 100.0))),
            ),
        ));
        let application = ApplicationContext::empty();
        let router = application.router::<Location>();
        let mut ui = UiRuntime::new();

        mount_nested_router(&ui, application.clone(), routes.clone());
        ui.run_effects();
        router.navigate(Location::new("/dialog"));
        assert!(!ui
            .apply_pending_updates(&crate::core::HostTree::new())
            .dirty_ids
            .is_empty());
        mount_nested_router(&ui, application, routes);

        assert_eq!(metadata_renders.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn redirect_replaces_the_location_without_adding_history() {
        let routes = create_router((redirect("/", "/community"), route("/community", empty)));
        let application = ApplicationContext::empty();
        let router = application.router::<Location>();
        let ui = UiRuntime::new();

        mount_nested_router(&ui, application, routes);
        ui.run_effects();

        assert_eq!(router.current().path(), "/community");
        assert!(!router.snapshot().can_back());
    }
}
