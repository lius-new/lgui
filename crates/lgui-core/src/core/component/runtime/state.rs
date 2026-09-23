use super::*;

pub struct RuntimeOutput {
    pub events: Vec<UiEvent>,
    pub handler_events: Vec<UiHandlerEvent>,
    pub action_events: Vec<UiActionEvent>,
    pub default_actions: Vec<UiDefaultAction>,
    pub dirty_bounds: Option<UiRect>,
    pub animation_changed: bool,
    pub route_changed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingUpdateOutput {
    pub dirty_ids: Vec<UiId>,
    pub focus_changed: bool,
    pub focus_events: Vec<UiEvent>,
    pub frame_requested: bool,
}

#[derive(Clone, Debug)]
pub struct UiDefaultAction {
    pub event_target: UiId,
    pub action_target: UiId,
    pub action: UiAction,
}

#[derive(Default)]
pub struct UiRuntime {
    pub(super) events: UiEventDispatcher,
    pub(super) animations: AnimationRegistry,
    pub(super) component_states: ComponentStateStore,
    pub(super) component_tree: ComponentTree,
    pub(super) contexts: ContextRegistry,
    pub(super) hook_states: HookStateStore,
    pub(super) hook_updates: Arc<UiUpdateQueue>,
    pub(super) task_spawner: Option<UiTaskSpawner>,
    pub(super) effects: EffectRegistry,
    pub(super) dirty: DirtyTracker,
    pub(super) frame_interval_ms: Option<u64>,
}

impl UiRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        let hook_updates = Arc::clone(&self.hook_updates);
        let task_spawner = self.task_spawner.clone();
        hook_updates.clear();
        *self = Self {
            hook_updates,
            task_spawner,
            ..Self::default()
        };
    }

    pub fn animations(&self) -> &AnimationRegistry {
        &self.animations
    }

    pub fn animations_mut(&mut self) -> &mut AnimationRegistry {
        &mut self.animations
    }

    pub fn component_states(&self) -> &ComponentStateStore {
        &self.component_states
    }

    pub fn component_tree(&self) -> &ComponentTree {
        &self.component_tree
    }

    pub fn frame_interval_ms(&self) -> Option<u64> {
        [
            self.frame_interval_ms,
            self.animations.is_running().then_some(16),
            self.component_states.requested_frame_interval_ms(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn invalidate_all_components(&self) {
        self.component_tree.mark_all_dirty();
    }

    pub fn contexts(&self) -> &ContextRegistry {
        &self.contexts
    }

    pub fn hook_states(&self) -> &HookStateStore {
        &self.hook_states
    }

    pub fn hook_updates(&self) -> &Arc<UiUpdateQueue> {
        &self.hook_updates
    }

    pub fn set_wake(&self, wake: UiWake) {
        self.hook_updates.set_wake(wake);
    }

    pub fn set_task_spawner(&mut self, spawner: UiTaskSpawner) {
        self.task_spawner = Some(spawner);
    }

    pub fn task_spawner(&self) -> Option<&UiTaskSpawner> {
        self.task_spawner.as_ref()
    }

    pub fn apply_pending_updates(&mut self, tree: &HostTree) -> PendingUpdateOutput {
        let dirty = self
            .hook_updates
            .apply(&self.hook_states, &self.component_tree);
        let focus_events = self
            .hook_updates
            .take_focus_request()
            .map(|target| self.focus_node_events(tree, &target))
            .unwrap_or_default();
        let focus_changed = !focus_events.is_empty();
        if focus_changed {
            // The caller schedules focus damage from PendingUpdateOutput. Do not leak the same
            // dirty event into the next unrelated native input pass.
            let _ = self.dirty.take();
        }
        let frame_requested = self.hook_updates.take_frame_request();
        PendingUpdateOutput {
            dirty_ids: dirty,
            focus_changed,
            focus_events,
            frame_requested,
        }
    }

    pub fn effects(&self) -> &EffectRegistry {
        &self.effects
    }

    pub fn run_effects(&self) {
        self.effects.run_pending();
    }

    pub fn interaction_state(&self) -> super::UiInteractionState {
        self.events.state()
    }

    pub fn clear_interaction_state(&mut self) -> bool {
        let state_changed = self.events.clear_interaction_state();
        let animation_changed = self.animations.clear_targets(&[
            AnimProperty::Hover,
            AnimProperty::Pressed,
            AnimProperty::Focus,
        ]);
        state_changed || animation_changed
    }

    pub fn clear_effects(&mut self) {
        self.effects.clear();
    }

    pub fn clear_hook_state(&mut self) {
        self.hook_updates.clear();
        self.hook_states.clear();
    }

    pub(crate) fn suspend_rendering(&mut self) {
        self.clear_interaction_state();
        self.dirty = DirtyTracker::default();
    }

    pub(super) fn mark_component_owners<'a>(
        &self,
        tree: &HostTree,
        ids: impl IntoIterator<Item = &'a UiId>,
    ) {
        for id in ids {
            if let Some(owner) = tree.node(id).and_then(|node| node.component_owner) {
                self.component_tree.mark_dirty(owner);
            }
        }
    }
}
