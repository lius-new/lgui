use std::sync::Arc;

use crate::RouterApplicationExt as _;

use crate::core::{
    component, context_provider, group, Element, Observable, ObservableListener, RenderCx, UiEffect,
};

use super::{
    super::{
        matcher::matches::{CurrentRouteMatch, RouteMatchesObservable},
        Location, RouteMatch, RouteMatches, Router,
    },
    builder::{DeclarativeRouter, ResolvedBranch},
};

impl<R> DeclarativeRouter<R>
where
    R: Default + Clone + PartialEq + Send + Sync + 'static,
{
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
