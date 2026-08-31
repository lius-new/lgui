use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use crate::{
    application::{AppView, ApplicationContext},
    core::{
        component, context_provider, group, text, Color, Element, HostTree, HostTreeBuilder,
        InputEvent, InteractionRole, Point, PointerButton, PointerData, RenderCx, RootComponent,
        TextStyle, UiElement, UiRect, UiRuntime, UiScale, VisualStyle,
    },
    router::{Location, RouteMatchHooks as _},
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
        scope("/community", route("events", empty)).handle(NavigationHandle::Section("community")),
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
        std::panic::catch_unwind(|| { create_router(route("/files/*path/more", empty)) }).is_err()
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
