use super::*;
use crate::{
    application::ApplicationContext,
    core::{
        component, content_text, context_provider, ComponentTree, HostTree, HostTreeBuilder,
        RootComponent, UiRect, UiRuntime, UiScale, UiTask,
    },
    events::{Event, EventKey},
};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};

#[derive(Clone)]
struct HookEvent;

const HOOK_EVENT: EventKey<HookEvent> = EventKey::new("test.hook");

impl Event for HookEvent {
    const NAME: &'static str = "test.hook";
}

#[allow(dead_code)]
fn async_event_hook_is_part_of_the_portable_api(cx: &mut RenderCx<'_, '_>) {
    cx.use_event_async_once::<HookEvent>(|_, _| async {});
    cx.listen_async(HOOK_EVENT.clone(), |_| async {});
}

struct EventRoot {
    application: ApplicationContext,
    mounted: bool,
    deliveries: Arc<AtomicUsize>,
}

impl RootComponent for EventRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        let content = if self.mounted {
            let deliveries = Arc::clone(&self.deliveries);
            component((), move |_cx, _| {
                let deliveries = Arc::clone(&deliveries);
                crate::events::listen(HOOK_EVENT.clone(), move |_| {
                    deliveries.fetch_add(1, Ordering::SeqCst);
                });
                content_text("mounted")
            })
        } else {
            content_text("unmounted")
        };
        context_provider(self.application, content)
    }
}

fn mount_event_root(ui: &UiRuntime, tree: HostTree, root: impl RootComponent) -> HostTree {
    let viewport = UiRect::new(0.0, 0.0, 10.0, 10.0);
    let interaction = ui.interaction_state();
    let mut builder = HostTreeBuilder::from_retained(tree);
    builder.mount(
        root,
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

const ASYNC_SOURCE: EventKey<usize> = EventKey::new("test.async.source");
const ASYNC_RESULT: EventKey<usize> = EventKey::new("test.async.result");

struct AsyncEventRoot {
    application: ApplicationContext,
    total: Arc<AtomicUsize>,
}

impl RootComponent for AsyncEventRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> crate::core::Element {
        let total = Arc::clone(&self.total);
        context_provider(
            self.application,
            component((), move |_cx, _| {
                crate::events::listen_async(ASYNC_SOURCE.clone(), |value| async move {
                    crate::events::emit(ASYNC_RESULT.clone(), value).await;
                });
                let total = Arc::clone(&total);
                crate::events::listen(ASYNC_RESULT.clone(), move |value| {
                    total.fetch_add(value, Ordering::SeqCst);
                });
                content_text("async listeners")
            }),
        )
    }
}

#[test]
fn equality_setter_is_directly_callable_and_try_update_enqueues_once() {
    let value = Arc::new(Mutex::new(1_u32));
    let writes = Arc::new(AtomicUsize::new(0));
    let set_writes = Arc::clone(&writes);
    let state = State {
        value: Arc::clone(&value),
        invalidate: Arc::new(move || {
            set_writes.fetch_add(1, Ordering::SeqCst);
        }),
    };
    let setter = StateSetter::new(state).with_equality();

    setter(1);
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert!(setter.try_update(|value| {
        *value += 1;
        true
    }));
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert_eq!(*value.lock().expect("test state value poisoned"), 2);
}

#[test]
fn stale_state_handle_cannot_dirty_a_remounted_component_generation() {
    let components = ComponentTree::new();
    let updates = Arc::new(super::super::UiUpdateQueue::new());
    let store = super::super::HookStateStore::new();

    components.begin_render();
    let old_owner = components.root(UiId::owned("state-owner"), "state owner");
    components.begin_component_execution(old_owner);
    components.finish_component(old_owner);
    components.end_render();

    let state_updates = Arc::clone(&updates);
    let state = State {
        value: Arc::new(Mutex::new(1_u32)),
        invalidate: Arc::new(move || {
            state_updates.invalidate(old_owner, UiId::owned("state-owner"));
        }),
    };

    components.begin_render();
    components.end_render();
    components.begin_render();
    let new_owner = components.root(UiId::owned("state-owner"), "state owner");
    components.begin_component_execution(new_owner);
    components.finish_component(new_owner);
    components.end_render();
    assert_ne!(old_owner, new_owner);

    state.set(2);

    assert!(updates.apply(&store, &components).is_empty());
    assert!(!components.is_dirty(new_owner));
}

#[test]
fn event_hook_unsubscribes_when_its_component_unmounts() {
    let ui = UiRuntime::new();
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    let deliveries = Arc::new(AtomicUsize::new(0));

    let tree = mount_event_root(
        &ui,
        HostTree::new(),
        EventRoot {
            application: application.clone(),
            mounted: true,
            deliveries: Arc::clone(&deliveries),
        },
    );
    ui.run_effects();
    assert_eq!(application.emit_keyed(&HOOK_EVENT, HookEvent), 1);
    assert_eq!(deliveries.load(Ordering::SeqCst), 1);

    ui.component_tree().mark_all_dirty();
    let _tree = mount_event_root(
        &ui,
        tree,
        EventRoot {
            application: application.clone(),
            mounted: false,
            deliveries: Arc::clone(&deliveries),
        },
    );
    ui.run_effects();

    assert_eq!(application.emit_keyed(&HOOK_EVENT, HookEvent), 0);
    assert_eq!(deliveries.load(Ordering::SeqCst), 1);
}

#[test]
fn async_listener_tasks_inherit_their_application_scope() {
    let ui = UiRuntime::new();
    let application = ApplicationContext::empty(crate::memory::test_memory_options());
    application.set_executor(Arc::new(|mut task: UiTask| {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(task.as_mut().poll(&mut context), Poll::Ready(())));
    }));
    let total = Arc::new(AtomicUsize::new(0));

    let _tree = mount_event_root(
        &ui,
        HostTree::new(),
        AsyncEventRoot {
            application: application.clone(),
            total: Arc::clone(&total),
        },
    );
    ui.run_effects();

    assert_eq!(application.emit_keyed(&ASYNC_SOURCE, 4), 1);
    assert_eq!(total.load(Ordering::SeqCst), 4);
}
