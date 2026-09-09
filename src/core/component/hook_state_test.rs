use super::*;

#[test]
fn queued_updates_are_batched_by_owner() {
    let store = HookStateStore::new();
    let queue = UiUpdateQueue::new();
    let components = ComponentTree::new();
    components.begin_render();
    let owner = components.root(UiId::owned("component"), "component");
    let hook = HookId::new(owner, 0, super::super::HookSlotKind::State);
    let invalidation_id = UiId::owned("component");
    assert_eq!(store.value(hook, || 1_i32), 1);

    queue.enqueue(owner, invalidation_id.clone(), hook, 2_i32);
    queue.enqueue_update(owner, invalidation_id.clone(), hook, |value: &mut i32| {
        *value += 3
    });

    assert_eq!(queue.apply(&store, &components), vec![invalidation_id]);
    assert_eq!(store.value(hook, || 0_i32), 5);
}

#[test]
fn stale_updates_do_not_recreate_unmounted_state() {
    let store = HookStateStore::new();
    let queue = UiUpdateQueue::new();
    let components = ComponentTree::new();
    components.begin_render();
    let owner = components.root(UiId::owned("removed"), "removed");
    let hook = HookId::new(owner, 0, super::super::HookSlotKind::State);

    queue.enqueue(owner, UiId::owned("removed"), hook, 4_i32);
    components.begin_render();
    components.end_render();

    assert!(queue.apply(&store, &components).is_empty());
    assert!(!store.contains(hook));
}

#[test]
fn explicit_frame_requests_are_coalesced_and_consumed_separately_from_state_updates() {
    let queue = UiUpdateQueue::new();

    queue.request_frame();
    queue.request_frame();

    assert!(!queue.is_empty());
    assert!(queue.take_frame_request());
    assert!(!queue.take_frame_request());
    assert!(queue.is_empty());
}

#[test]
fn abort_render_removes_hooks_created_by_the_abandoned_frame() {
    let store = HookStateStore::new();
    let components = ComponentTree::new();
    components.begin_render();
    let owner = components.root(UiId::owned("component"), "component");
    components.finish_component(owner);
    components.end_render();
    let hook = HookId::new(owner, 0, super::super::HookSlotKind::State);

    store.begin_render();
    assert_eq!(store.value(hook, || 7_u32), 7);
    store.abort_render(&components);

    assert!(!store.contains(hook));
}
