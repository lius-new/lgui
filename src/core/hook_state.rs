use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, RwLock},
};

use super::{ComponentId, ComponentTree, HookId, UiId};

type HookValue = Box<dyn Any>;
type PendingHookUpdate = Box<dyn FnOnce(&HookStateStore) -> bool + Send + 'static>;
pub type UiWake = Arc<dyn Fn() + Send + Sync + 'static>;

struct QueuedHookUpdate {
    owner: ComponentId,
    invalidation_id: UiId,
    apply: PendingHookUpdate,
}

#[derive(Default)]
pub struct HookStateStore {
    values: RefCell<HashMap<HookId, HookValue>>,
    seen: RefCell<HashSet<HookId>>,
    inserted: RefCell<HashSet<HookId>>,
    tracking_render: Cell<bool>,
}

impl HookStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_render(&self) {
        if self.tracking_render.get() {
            self.abort_render_state();
        }
        self.seen.borrow_mut().clear();
        self.inserted.borrow_mut().clear();
        self.tracking_render.set(true);
    }

    pub fn end_render(&self, components: &ComponentTree) {
        self.values
            .borrow_mut()
            .retain(|id, _| components.is_alive(id.component()));
        self.tracking_render.set(false);
        self.seen.borrow_mut().clear();
        self.inserted.borrow_mut().clear();
    }

    pub fn abort_render(&self, _components: &ComponentTree) {
        self.abort_render_state();
    }

    pub fn value<T>(&self, id: HookId, initial: impl FnOnce() -> T) -> T
    where
        T: Clone + 'static,
    {
        self.mark_seen(id);
        let mut values = self.values.borrow_mut();
        let existed = values.contains_key(&id);
        let value = values.entry(id).or_insert_with(|| Box::new(initial()));
        if self.tracking_render.get() && !existed {
            self.inserted.borrow_mut().insert(id);
        }
        value
            .downcast_ref::<T>()
            .unwrap_or_else(|| panic!("hook state type mismatch for `{id}`"))
            .clone()
    }

    pub fn update_existing<T>(&self, id: HookId, update: impl FnOnce(&mut T)) -> bool
    where
        T: 'static,
    {
        let mut values = self.values.borrow_mut();
        let Some(value) = values.get_mut(&id) else {
            return false;
        };
        let value = value
            .downcast_mut::<T>()
            .unwrap_or_else(|| panic!("hook state type mismatch for `{id}`"));
        update(value);
        true
    }

    pub fn contains(&self, id: HookId) -> bool {
        self.values.borrow().contains_key(&id)
    }

    pub fn clear(&self) {
        self.values.borrow_mut().clear();
        self.seen.borrow_mut().clear();
        self.inserted.borrow_mut().clear();
        self.tracking_render.set(false);
    }

    fn mark_seen(&self, id: HookId) {
        if self.tracking_render.get() {
            self.seen.borrow_mut().insert(id);
        }
    }

    fn abort_render_state(&self) {
        let inserted = std::mem::take(&mut *self.inserted.borrow_mut());
        self.values
            .borrow_mut()
            .retain(|id, _| !inserted.contains(id));
        self.tracking_render.set(false);
        self.seen.borrow_mut().clear();
    }
}

#[derive(Default)]
pub struct UiUpdateQueue {
    pending: Mutex<Vec<QueuedHookUpdate>>,
    focus_request: Mutex<Option<UiId>>,
    wake: RwLock<Option<UiWake>>,
}

impl UiUpdateQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_wake(&self, wake: UiWake) {
        *self.wake.write().expect("hook wake lock poisoned") = Some(wake);
    }

    pub fn clear_wake(&self) {
        *self.wake.write().expect("hook wake lock poisoned") = None;
    }

    pub fn invalidate(&self, owner: ComponentId, invalidation_id: UiId) {
        self.pending
            .lock()
            .expect("hook update queue poisoned")
            .push(QueuedHookUpdate {
                owner,
                invalidation_id,
                apply: Box::new(|_| true),
            });
        self.wake();
    }

    pub fn request_focus(&self, target: UiId) {
        *self
            .focus_request
            .lock()
            .expect("hook focus request lock poisoned") = Some(target);
        self.wake();
    }

    pub fn take_focus_request(&self) -> Option<UiId> {
        self.focus_request
            .lock()
            .expect("hook focus request lock poisoned")
            .take()
    }

    pub fn enqueue<T>(&self, owner: ComponentId, invalidation_id: UiId, id: HookId, value: T)
    where
        T: Send + 'static,
    {
        self.enqueue_update(owner, invalidation_id, id, move |current: &mut T| {
            *current = value
        });
    }

    pub fn enqueue_update<T>(
        &self,
        owner: ComponentId,
        invalidation_id: UiId,
        id: HookId,
        update: impl FnOnce(&mut T) + Send + 'static,
    ) where
        T: 'static,
    {
        self.pending
            .lock()
            .expect("hook update queue poisoned")
            .push(QueuedHookUpdate {
                owner,
                invalidation_id,
                apply: Box::new(move |store| store.update_existing(id, update)),
            });
        self.wake();
    }

    pub fn apply(&self, store: &HookStateStore, components: &ComponentTree) -> Vec<UiId> {
        let pending =
            std::mem::take(&mut *self.pending.lock().expect("hook update queue poisoned"));
        let mut dirty = HashSet::new();
        for update in pending {
            if components.is_alive(update.owner) && (update.apply)(store) {
                components.mark_dirty(update.owner);
                dirty.insert(update.invalidation_id);
            }
        }
        dirty.into_iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.pending
            .lock()
            .expect("hook update queue poisoned")
            .is_empty()
            && self
                .focus_request
                .lock()
                .expect("hook focus request lock poisoned")
                .is_none()
    }

    pub fn clear(&self) {
        self.pending
            .lock()
            .expect("hook update queue poisoned")
            .clear();
        self.focus_request
            .lock()
            .expect("hook focus request lock poisoned")
            .take();
    }

    fn wake(&self) {
        if let Some(wake) = self
            .wake
            .read()
            .expect("hook wake lock poisoned")
            .as_ref()
            .cloned()
        {
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
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
}
