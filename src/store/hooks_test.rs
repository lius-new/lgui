use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use crate::core::{
    component, content_text, context_provider, HostTreeBuilder, RenderCx, RootComponent, UiRect,
    UiRuntime, UiScale,
};

use super::*;
use crate::{resources::Resources, store::create};

struct CounterStore {
    count: i32,
    label: &'static str,
}

fn create_counter() -> CounterStore {
    CounterStore {
        count: 1,
        label: "counter",
    }
}

fn increment(store: &mut CounterStore) {
    store.count += 1;
}

const COUNTER: StoreDefinition<CounterStore> = create("test.counter.hook", create_counter);
const INCREMENT: StoreAction<CounterStore> = COUNTER.action(increment);

struct CounterRoot {
    stores: StoreContext,
    executions: Arc<AtomicUsize>,
    action: Arc<Mutex<Option<BoundStoreAction<CounterStore>>>>,
}

impl RootComponent for CounterRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        context_provider(
            self.stores,
            component((), move |cx, _| {
                self.executions.fetch_add(1, Ordering::SeqCst);
                let count = COUNTER.select(cx, |store| store.count);
                *self.action.lock().expect("counter action poisoned") = Some(INCREMENT.bind(cx));
                content_text(count.to_string())
            }),
        )
    }
}

fn mount(
    ui: &UiRuntime,
    stores: StoreContext,
    executions: Arc<AtomicUsize>,
    action: Arc<Mutex<Option<BoundStoreAction<CounterStore>>>>,
) {
    let viewport = UiRect::new(0.0, 0.0, 10.0, 10.0);
    let interaction = ui.interaction_state();
    let mut builder = HostTreeBuilder::new();
    builder.mount(
        CounterRoot {
            stores,
            executions,
            action,
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
fn selectors_ignore_unrelated_fields_and_bound_actions_batch_component_updates() {
    let stores = Arc::new(StoreRuntime::new(Resources::new()));
    let context = StoreContext::new(Arc::clone(&stores));
    let mut ui = UiRuntime::new();
    let executions = Arc::new(AtomicUsize::new(0));
    let action = Arc::new(Mutex::new(None));

    mount(
        &ui,
        context.clone(),
        Arc::clone(&executions),
        Arc::clone(&action),
    );
    ui.run_effects();
    assert_eq!(executions.load(Ordering::SeqCst), 1);

    stores.update_defined(COUNTER, "label", |store| store.label = "renamed");
    assert!(ui
        .apply_pending_updates(&crate::core::HostTree::new())
        .dirty_ids
        .is_empty());
    mount(
        &ui,
        context.clone(),
        Arc::clone(&executions),
        Arc::clone(&action),
    );
    assert_eq!(executions.load(Ordering::SeqCst), 1);

    let increment = action
        .lock()
        .expect("counter action poisoned")
        .clone()
        .expect("counter action missing");
    increment.call();
    increment.call();
    assert_eq!(
        ui.apply_pending_updates(&crate::core::HostTree::new())
            .dirty_ids
            .len(),
        1
    );
    mount(&ui, context, Arc::clone(&executions), Arc::clone(&action));

    assert_eq!(executions.load(Ordering::SeqCst), 2);
    assert_eq!(stores.read_defined(COUNTER, |store| store.count), 3);
}
