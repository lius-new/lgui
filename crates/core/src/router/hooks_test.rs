use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use crate::core::{
    component, content_text, group, HostTreeBuilder, RenderCx, RootComponent, UiRect, UiRuntime,
    UiScale,
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Home,
    Settings,
}

struct RouterRoot {
    router: Router<Route>,
    routed_executions: Arc<AtomicUsize>,
    unrelated_executions: Arc<AtomicUsize>,
}

impl RootComponent for RouterRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        let router = self.router;
        let routed_executions = self.routed_executions;
        let unrelated_executions = self.unrelated_executions;
        group(UiRect::new(0.0, 0.0, 10.0, 10.0)).content((
            component((), move |cx, _| {
                routed_executions.fetch_add(1, Ordering::SeqCst);
                let snapshot = cx.use_router_snapshot(router.clone());
                content_text(format!("{:?}", snapshot.current()))
            }),
            component((), move |_cx, _| {
                unrelated_executions.fetch_add(1, Ordering::SeqCst);
                content_text("unrelated")
            }),
        ))
    }
}

fn mount(
    ui: &UiRuntime,
    router: Router<Route>,
    routed_executions: Arc<AtomicUsize>,
    unrelated_executions: Arc<AtomicUsize>,
) {
    let viewport = UiRect::new(0.0, 0.0, 10.0, 10.0);
    let interaction = ui.interaction_state();
    let mut builder = HostTreeBuilder::new();
    builder.mount(
        RouterRoot {
            router,
            routed_executions,
            unrelated_executions,
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
}

#[test]
fn route_changes_invalidate_only_the_subscribed_component() {
    let router = Router::new(Route::Home);
    let mut ui = UiRuntime::new();
    let routed_executions = Arc::new(AtomicUsize::new(0));
    let unrelated_executions = Arc::new(AtomicUsize::new(0));

    mount(
        &ui,
        router.clone(),
        Arc::clone(&routed_executions),
        Arc::clone(&unrelated_executions),
    );
    ui.run_effects();
    assert_eq!(routed_executions.load(Ordering::SeqCst), 1);
    assert_eq!(unrelated_executions.load(Ordering::SeqCst), 1);

    router.navigate(Route::Settings);
    assert_eq!(
        ui.apply_pending_updates(&crate::core::HostTree::new())
            .dirty_ids
            .len(),
        1
    );
    mount(
        &ui,
        router,
        Arc::clone(&routed_executions),
        Arc::clone(&unrelated_executions),
    );

    assert_eq!(routed_executions.load(Ordering::SeqCst), 2);
    assert_eq!(unrelated_executions.load(Ordering::SeqCst), 1);
}
