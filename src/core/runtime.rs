use super::{
    apply_events_to_animations, AnimProperty, AnimationRegistry, ComponentStateStore,
    ComponentTree, ContextRegistry, DirtyTracker, EffectRegistry, HookStateStore, HostTree,
    InputEvent, KeyCode, UiAction, UiActionEvent, UiEvent, UiEventDispatcher, UiEventPayload,
    UiHandlerEvent, UiId, UiRect, UiTaskSpawner, UiUpdateQueue, UiWake,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const FOCUS_TRAVERSAL_ACTION: &str = "ui.focus.traverse";

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
}

#[derive(Clone, Debug)]
pub struct UiDefaultAction {
    pub event_target: UiId,
    pub action_target: UiId,
    pub action: UiAction,
}

#[derive(Default)]
pub struct UiRuntime {
    events: UiEventDispatcher,
    animations: AnimationRegistry,
    component_states: ComponentStateStore,
    component_tree: ComponentTree,
    contexts: ContextRegistry,
    hook_states: HookStateStore,
    hook_updates: Arc<UiUpdateQueue>,
    task_spawner: Option<UiTaskSpawner>,
    effects: EffectRegistry,
    dirty: DirtyTracker,
    previous_bounds: HashMap<UiId, UiRect>,
    current_tree: HostTree,
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

    pub fn apply_pending_updates(&mut self) -> PendingUpdateOutput {
        let dirty = self
            .hook_updates
            .apply(&self.hook_states, &self.component_tree);
        self.dirty.mark_animation_ids(dirty.iter().cloned());
        let focus_changed = self
            .hook_updates
            .take_focus_request()
            .is_some_and(|target| self.focus_node(&target));
        PendingUpdateOutput {
            dirty_ids: dirty,
            focus_changed,
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
        self.previous_bounds.clear();
        self.current_tree = HostTree::new();
        self.dirty = DirtyTracker::default();
    }

    pub fn handle_input(&mut self, tree: &HostTree, input: InputEvent) -> RuntimeOutput {
        self.reconcile_tree(tree);
        let focus_traversal = match &input {
            InputEvent::KeyDown {
                key: KeyCode::Tab,
                modifiers,
            } => Some(modifiers.shift),
            _ => None,
        };
        let mut events = self.events.dispatch(tree, input);
        let action_events = Vec::new();
        let mut default_actions = Vec::new();
        events.retain(|event| {
            match event {
                UiEvent::Wheel { hit, delta_y } => {
                    let Some(action) = hit.action.as_ref() else {
                        return true;
                    };
                    let action = action.clone().payload(delta_y.to_string());
                    let target = hit.action_target.as_ref().unwrap_or(&hit.id);
                    default_actions.push(UiDefaultAction {
                        event_target: hit.id.clone(),
                        action_target: target.clone(),
                        action,
                    });
                }
                UiEvent::Clicked(hit) => {
                    if let Some(action) = hit.action.as_ref() {
                        let target = hit.action_target.as_ref().unwrap_or(&hit.id);
                        default_actions.push(UiDefaultAction {
                            event_target: hit.id.clone(),
                            action_target: target.clone(),
                            action: action.clone(),
                        });
                    }
                }
                UiEvent::TextInput { target, text } => {
                    default_actions.push(UiDefaultAction {
                        event_target: target.clone(),
                        action_target: target.clone(),
                        action: UiAction::new("text.input").payload(text.clone()),
                    });
                }
                UiEvent::Backspace { target } => {
                    default_actions.push(UiDefaultAction {
                        event_target: target.clone(),
                        action_target: target.clone(),
                        action: UiAction::new("text.backspace"),
                    });
                }
                UiEvent::KeyDown {
                    target,
                    key,
                    modifiers,
                } => {
                    let action = match key {
                        KeyCode::ArrowLeft => {
                            Some(UiAction::new("text.move.left").payload(if modifiers.shift {
                                "extend"
                            } else {
                                "collapse"
                            }))
                        }
                        KeyCode::ArrowRight => Some(UiAction::new("text.move.right").payload(
                            if modifiers.shift {
                                "extend"
                            } else {
                                "collapse"
                            },
                        )),
                        KeyCode::ArrowUp => {
                            Some(UiAction::new("text.move.up").payload(if modifiers.shift {
                                "extend"
                            } else {
                                "collapse"
                            }))
                        }
                        KeyCode::ArrowDown => {
                            Some(UiAction::new("text.move.down").payload(if modifiers.shift {
                                "extend"
                            } else {
                                "collapse"
                            }))
                        }
                        KeyCode::Enter => Some(UiAction::new("text.input").payload("\n")),
                        KeyCode::A if modifiers.ctrl => Some(UiAction::new("text.select.all")),
                        KeyCode::C if modifiers.ctrl => Some(UiAction::new("text.copy")),
                        KeyCode::V if modifiers.ctrl => Some(UiAction::new("text.paste")),
                        _ => None,
                    };
                    if let Some(action) = action {
                        default_actions.push(UiDefaultAction {
                            event_target: target.clone(),
                            action_target: target.clone(),
                            action,
                        });
                    }
                }
                UiEvent::PointerPressed { hit, point } => {
                    let payload = format!("{},{}", point.x - hit.rect.left, point.y - hit.rect.top);
                    let target = hit.action_target.as_ref().unwrap_or(&hit.id);
                    if self.component_states.contains(target) {
                        default_actions.push(UiDefaultAction {
                            event_target: hit.id.clone(),
                            action_target: target.clone(),
                            action: UiAction::new(super::POINTER_DOWN_ACTION).payload(payload),
                        });
                    }
                }
                UiEvent::PointerDragged { hit, point } => {
                    let payload = format!("{},{}", point.x - hit.rect.left, point.y - hit.rect.top);
                    let target = hit.action_target.as_ref().unwrap_or(&hit.id);
                    if self.component_states.contains(target) {
                        default_actions.push(UiDefaultAction {
                            event_target: hit.id.clone(),
                            action_target: target.clone(),
                            action: UiAction::new(super::POINTER_DRAG_ACTION).payload(payload),
                        });
                    }
                }
                UiEvent::PointerReleased { hit, point } => {
                    let payload = format!("{},{}", point.x - hit.rect.left, point.y - hit.rect.top);
                    let target = hit.action_target.as_ref().unwrap_or(&hit.id);
                    if self.component_states.contains(target) {
                        default_actions.push(UiDefaultAction {
                            event_target: hit.id.clone(),
                            action_target: target.clone(),
                            action: UiAction::new(super::POINTER_UP_ACTION).payload(payload),
                        });
                    }
                }
                _ => {}
            }
            true
        });
        if let Some(reverse) = focus_traversal {
            let event_target = self
                .events
                .state()
                .focused
                .or_else(|| tree.active_focus_scope_id())
                .or_else(|| tree.focusable_hits().into_iter().next().map(|hit| hit.id));
            if let Some(event_target) = event_target {
                default_actions.push(UiDefaultAction {
                    event_target: event_target.clone(),
                    action_target: event_target,
                    action: UiAction::new(FOCUS_TRAVERSAL_ACTION).payload(if reverse {
                        "reverse"
                    } else {
                        "forward"
                    }),
                });
            }
        }
        let handler_events = events
            .iter()
            .flat_map(|event| tree.handler_events(event))
            .collect();
        for event in events.iter().cloned() {
            self.dirty.mark_event(event);
        }
        let animation_changed =
            apply_events_to_animations(tree, &mut self.animations, events.iter().cloned());
        if animation_changed {
            self.mark_component_owners(tree, events.iter().flat_map(event_target_ids));
        }
        let dirty_bounds = self.dirty.take().bounds(tree);
        RuntimeOutput {
            events,
            handler_events,
            action_events,
            default_actions,
            dirty_bounds,
            animation_changed,
            route_changed: false,
        }
    }

    pub fn advance(&mut self, tree: &HostTree, elapsed_ms: f32) -> RuntimeOutput {
        self.reconcile_tree(tree);
        let animation_changed = self.animations.advance(elapsed_ms);
        let animation_ids = self.animations.take_dirty_ids();
        self.mark_component_owners(tree, animation_ids.iter());
        self.dirty.mark_animation_ids(animation_ids);
        let component_dirty_ids = self.component_states.advance(elapsed_ms);
        let component_changed = !component_dirty_ids.is_empty();
        self.mark_component_owners(tree, component_dirty_ids.iter());
        let mut route_changed = false;
        for id in &component_dirty_ids {
            route_changed |= self.component_states.take_route_invalidation(id);
        }
        self.dirty.mark_animation_ids(component_dirty_ids);
        let dirty_bounds = self.dirty.take().bounds(tree);
        RuntimeOutput {
            events: Vec::new(),
            handler_events: Vec::new(),
            action_events: Vec::new(),
            default_actions: Vec::new(),
            dirty_bounds,
            animation_changed: animation_changed || component_changed,
            route_changed,
        }
    }

    pub fn handle_default_action(&mut self, pending: UiDefaultAction) -> RuntimeOutput {
        let tree = self.current_tree.clone();
        if pending.action.id().as_str() == FOCUS_TRAVERSAL_ACTION {
            let events = self
                .events
                .focus_adjacent(&tree, pending.action.payload_value() == Some("reverse"));
            let handler_events = events
                .iter()
                .flat_map(|event| tree.handler_events(event))
                .collect();
            for event in events.iter().cloned() {
                self.dirty.mark_event(event);
            }
            let animation_changed =
                apply_events_to_animations(&tree, &mut self.animations, events.iter().cloned());
            if animation_changed {
                self.mark_component_owners(&tree, events.iter().flat_map(event_target_ids));
            }
            let dirty_bounds = self.dirty.take().bounds(&tree);
            return RuntimeOutput {
                events,
                handler_events,
                action_events: Vec::new(),
                default_actions: Vec::new(),
                dirty_bounds,
                animation_changed,
                route_changed: false,
            };
        }
        let mut action_events = Vec::new();
        let (_, changed, route_changed) = self.apply_component_action(
            &tree,
            &pending.action_target,
            &pending.action,
            &mut action_events,
        );
        let handler_events = changed
            .then(|| {
                tree.handler_event(
                    &pending.action_target,
                    UiEventPayload::Change {
                        value: pending.action.payload_value().map(str::to_owned),
                    },
                )
            })
            .flatten()
            .into_iter()
            .collect();
        let dirty_bounds = self.dirty.take().bounds(&tree);
        RuntimeOutput {
            events: Vec::new(),
            handler_events,
            action_events,
            default_actions: Vec::new(),
            dirty_bounds,
            animation_changed: changed,
            route_changed,
        }
    }

    fn apply_component_action(
        &mut self,
        tree: &HostTree,
        target: &UiId,
        action: &UiAction,
        action_events: &mut Vec<UiActionEvent>,
    ) -> (bool, bool, bool) {
        let outcome = self.component_states.handle_action(target, action);
        if outcome.changed {
            self.dirty.mark_id(target.clone());
        }
        for action in outcome.events {
            let Some(handler) = tree.action_handler(target, action.id()) else {
                continue;
            };
            action_events.push(UiActionEvent {
                target: target.clone(),
                action,
                handler,
            });
        }
        let route_changed =
            outcome.changed && self.component_states.take_route_invalidation(target);
        (outcome.handled, outcome.changed, route_changed)
    }

    pub fn sync_tree_animation_targets(&mut self, tree: &HostTree) -> bool {
        let mut changed = false;
        let interaction_events = self.events.sync_interaction_for_tree(tree);
        self.mark_component_owners(tree, interaction_events.iter().flat_map(event_target_ids));
        for event in interaction_events.iter().cloned() {
            self.dirty.mark_event(event);
        }
        changed |=
            apply_events_to_animations(tree, &mut self.animations, interaction_events.into_iter());
        let present_ids: HashSet<UiId> = tree.nodes().iter().map(|node| node.id.clone()).collect();
        changed |= self.animations.clear_absent_values(
            &present_ids,
            &[
                AnimProperty::Hover,
                AnimProperty::Active,
                AnimProperty::Pressed,
                AnimProperty::Focus,
            ],
        );
        for node in tree.nodes() {
            for (property, active) in node.animation_targets.iter().copied() {
                for binding in node
                    .animation_bindings
                    .iter()
                    .copied()
                    .filter(|binding| binding.property == property)
                {
                    if self
                        .animations
                        .sync_binding_target(node.id.clone(), binding, active)
                    {
                        if let Some(owner) = node.component_owner {
                            self.component_tree.mark_dirty(owner);
                        }
                        self.dirty.mark_id(node.id.clone());
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub fn sync_tree_focus(&mut self, tree: &HostTree) -> bool {
        let events = self.events.sync_focus_for_tree(tree);
        if events.is_empty() {
            return false;
        }
        self.mark_component_owners(tree, events.iter().flat_map(event_target_ids));
        for event in &events {
            self.dirty.mark_event(event.clone());
        }
        apply_events_to_animations(tree, &mut self.animations, events);
        true
    }

    pub fn focus_node(&mut self, id: &UiId) -> bool {
        let events = self.events.focus_node(&self.current_tree, id);
        if events.is_empty() {
            return false;
        }
        self.mark_component_owners(&self.current_tree, events.iter().flat_map(event_target_ids));
        for event in &events {
            self.dirty.mark_event(event.clone());
        }
        apply_events_to_animations(&self.current_tree, &mut self.animations, events);
        true
    }

    pub fn reconcile_tree(&mut self, tree: &HostTree) {
        self.current_tree = tree.clone();
        let mut next_bounds = HashMap::new();
        for node in tree.nodes() {
            next_bounds.insert(node.id.clone(), node.paint_bounds);
        }
        for (id, old_bounds) in &self.previous_bounds {
            match next_bounds.get(id) {
                Some(new_bounds) if new_bounds != old_bounds => {
                    self.dirty.mark_rect(old_bounds.union(*new_bounds));
                }
                None => self.dirty.mark_rect(*old_bounds),
                _ => {}
            }
        }
        self.previous_bounds = next_bounds;
    }

    fn mark_component_owners<'a>(&self, tree: &HostTree, ids: impl IntoIterator<Item = &'a UiId>) {
        for id in ids {
            if let Some(owner) = tree.node(id).and_then(|node| node.component_owner) {
                self.component_tree.mark_dirty(owner);
            }
        }
    }
}

fn event_target_ids(event: &UiEvent) -> Vec<&UiId> {
    match event {
        UiEvent::HoverChanged { previous, current }
        | UiEvent::PressedChanged { previous, current } => previous
            .iter()
            .chain(current.iter().map(|hit| &hit.id))
            .collect(),
        UiEvent::Clicked(hit)
        | UiEvent::Wheel { hit, .. }
        | UiEvent::PointerPressed { hit, .. }
        | UiEvent::PointerMoved { hit, .. }
        | UiEvent::PointerDragged { hit, .. }
        | UiEvent::PointerReleased { hit, .. } => vec![&hit.id],
        UiEvent::TextInput { target, .. }
        | UiEvent::ImeStarted { target }
        | UiEvent::ImeUpdated { target, .. }
        | UiEvent::ImeEnded { target }
        | UiEvent::Backspace { target }
        | UiEvent::KeyDown { target, .. } => vec![target],
        UiEvent::FocusChanged { current, previous } => previous
            .iter()
            .chain(current.iter().map(|hit| &hit.id))
            .collect(),
        UiEvent::PointerLeft { previous } => previous.iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::super::{
        AnimationBinding, ComponentActionOutcome, ComponentState, InputEvent, InteractionRole,
        Point, PointerButton, UiNode, UiNodeKind, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION,
        POINTER_UP_ACTION,
    };
    use super::*;

    #[test]
    fn tree_sync_drops_hover_for_removed_nodes() {
        let button_id = UiId::new("start");
        let button_tree = interactive_button_tree(button_id.clone());
        let empty_tree = HostTree::new();
        let mut runtime = UiRuntime::new();

        runtime.handle_input(&button_tree, InputEvent::PointerMove(Point::new(10, 10)));
        runtime.advance(&button_tree, 1000.0);

        assert_eq!(runtime.interaction_state().hovered, Some(button_id.clone()));
        assert_eq!(
            runtime
                .animations()
                .value(button_id.clone(), AnimProperty::Hover),
            1.0
        );

        assert!(runtime.sync_tree_animation_targets(&empty_tree));

        assert_eq!(runtime.interaction_state().hovered, None);
        assert_eq!(
            runtime.animations().value(button_id, AnimProperty::Hover),
            0.0
        );
    }

    #[test]
    fn tree_sync_drops_active_animation_for_removed_nodes() {
        let switch_id = UiId::new("switch");
        let mut switch_tree = HostTree::new();
        switch_tree.push(
            UiNode::new(
                switch_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0, 0, 46, 24),
            )
            .animation(AnimationBinding::new(AnimProperty::Active, 0.0, 1.0))
            .animation_target(AnimProperty::Active, true),
        );
        let mut runtime = UiRuntime::new();

        assert!(runtime.sync_tree_animation_targets(&switch_tree));
        assert_eq!(
            runtime
                .animations()
                .value(switch_id.clone(), AnimProperty::Active),
            1.0
        );

        assert!(runtime.sync_tree_animation_targets(&HostTree::new()));
        assert_eq!(
            runtime.animations().value(switch_id, AnimProperty::Active),
            0.0
        );
    }

    #[test]
    fn animation_target_changes_invalidate_the_owning_component() {
        let mut runtime = UiRuntime::new();
        let components = runtime.component_tree();
        components.begin_render();
        let owner = components.root(UiId::owned("animated-owner"), "animated-owner");
        components.begin_component_execution(owner);
        components.finish_component(owner);
        components.end_render();
        assert!(!components.is_dirty(owner));

        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                UiId::owned("animated-node"),
                UiNodeKind::Panel,
                UiRect::new(0, 0, 20, 20),
            )
            .component_owner(owner)
            .animation(AnimationBinding::new(AnimProperty::Active, 0.0, 1.0))
            .animation_target(AnimProperty::Active, true),
        );

        assert!(runtime.sync_tree_animation_targets(&tree));
        assert!(runtime.component_tree().is_dirty(owner));
    }

    #[test]
    fn pointer_down_on_empty_space_clears_focus() {
        let button_id = UiId::new("focus-target");
        let tree = interactive_button_tree(button_id.clone());
        let mut runtime = UiRuntime::new();

        runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                point: Point::new(10, 10),
                button: PointerButton::Left,
            },
        );
        assert_eq!(runtime.interaction_state().focused, Some(button_id));

        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                point: Point::new(200, 200),
                button: PointerButton::Left,
            },
        );
        assert_eq!(runtime.interaction_state().focused, None);
        assert!(output.events.iter().any(|event| matches!(
            event,
            UiEvent::FocusChanged {
                previous: Some(_),
                current: None
            }
        )));
    }

    #[test]
    fn focus_scope_traps_tab_and_restores_the_previous_focus() {
        let background = UiId::owned("background");
        let scope = UiId::owned("modal");
        let first = UiId::owned("modal.first");
        let second = UiId::owned("modal.second");
        let mut base_tree = HostTree::new();
        base_tree.push(
            UiNode::new(
                background.clone(),
                UiNodeKind::Button,
                UiRect::new(0, 0, 40, 20),
            )
            .interaction(InteractionRole::Button),
        );
        let mut modal_tree = base_tree.clone();
        modal_tree.push(
            UiNode::new(
                scope.clone(),
                UiNodeKind::Group,
                UiRect::new(0, 0, 100, 100),
            )
            .focus_scope(true),
        );
        for (id, rect) in [
            (first.clone(), UiRect::new(10, 10, 40, 30)),
            (second.clone(), UiRect::new(50, 10, 80, 30)),
        ] {
            modal_tree.push(
                UiNode::new(id, UiNodeKind::Button, rect)
                    .parent(scope.clone())
                    .interaction(InteractionRole::Button),
            );
        }
        let mut runtime = UiRuntime::new();
        runtime.reconcile_tree(&base_tree);
        assert!(runtime.focus_node(&background));

        assert!(runtime.sync_tree_focus(&modal_tree));
        assert_eq!(runtime.interaction_state().focused, Some(first.clone()));

        let output = runtime.handle_input(
            &modal_tree,
            InputEvent::KeyDown {
                key: KeyCode::Tab,
                modifiers: super::super::KeyModifiers::default(),
            },
        );
        assert_eq!(runtime.interaction_state().focused, Some(first));
        assert_eq!(output.default_actions.len(), 1);
        runtime.handle_default_action(output.default_actions[0].clone());
        assert_eq!(runtime.interaction_state().focused, Some(second));

        assert!(runtime.sync_tree_focus(&base_tree));
        assert_eq!(runtime.interaction_state().focused, Some(background));
    }

    #[test]
    fn tab_focus_traversal_is_deferred_until_after_key_handlers() {
        let id = UiId::owned("focusable");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(id.clone(), UiNodeKind::Button, UiRect::new(0, 0, 40, 20))
                .interaction(InteractionRole::Button)
                .on_event(super::super::UiEventKind::KeyDown, |context, _| {
                    context.prevent_default();
                }),
        );
        let mut runtime = UiRuntime::new();
        runtime.reconcile_tree(&tree);
        assert!(runtime.focus_node(&id));

        let output = runtime.handle_input(
            &tree,
            InputEvent::KeyDown {
                key: KeyCode::Tab,
                modifiers: super::super::KeyModifiers::default(),
            },
        );

        assert_eq!(output.handler_events.len(), 1);
        assert_eq!(output.default_actions.len(), 1);
        assert_eq!(runtime.interaction_state().focused, Some(id));
        let mut context = super::super::UiEventContext::new(
            crate::application::ApplicationContext::empty(),
            crate::application::WindowId::new("test"),
        );
        for handler in &output.handler_events[0].bubble_handlers {
            handler(&mut context, &output.handler_events[0].payload);
        }
        assert!(context.default_prevented());
    }

    #[test]
    fn pending_focus_request_targets_the_runtime_that_created_the_handle() {
        let target = UiId::new("queued-focus-target");
        let tree = interactive_button_tree(target.clone());
        let mut runtime = UiRuntime::new();
        runtime.reconcile_tree(&tree);

        runtime.hook_updates().request_focus(target.clone());
        let output = runtime.apply_pending_updates();

        assert!(output.focus_changed);
        assert_eq!(runtime.interaction_state().focused, Some(target));
    }

    #[test]
    fn pointer_drag_actions_keep_the_pressed_hit_outside_its_bounds() {
        let slider_id = UiId::new("slider");
        let tree = interactive_button_tree(slider_id.clone());
        let mut runtime = UiRuntime::new();
        runtime
            .component_states()
            .with_mut(&slider_id, |state: &mut PointerActionState| {
                state.points.clear();
            });

        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                point: Point::new(10, 10),
                button: PointerButton::Left,
            },
        );
        for action in output.default_actions {
            runtime.handle_default_action(action);
        }
        let output = runtime.handle_input(&tree, InputEvent::PointerMove(Point::new(180, 10)));
        for action in output.default_actions {
            runtime.handle_default_action(action);
        }
        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerUp {
                point: Point::new(180, 10),
                button: PointerButton::Left,
            },
        );
        for action in output.default_actions {
            runtime.handle_default_action(action);
        }

        let points = runtime
            .component_states()
            .with(&slider_id, |state: &PointerActionState| {
                state.points.clone()
            })
            .expect("pointer state");
        assert_eq!(points, vec![(10, 10), (180, 10), (180, 10)]);
    }

    #[test]
    fn component_actions_route_semantic_events_to_the_target_node() {
        let component_id = UiId::new("semantic-component");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                component_id.clone(),
                UiNodeKind::Button,
                UiRect::new(0, 0, 100, 40),
            )
            .interaction(InteractionRole::Button)
            .click_action(UiAction::new("component.choose"))
            .on_action("component.change", |_context, _action| {}),
        );
        let mut runtime = UiRuntime::new();
        runtime
            .component_states()
            .with_mut(&component_id, |_state: &mut SemanticActionState| {});

        runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                point: Point::new(10, 10),
                button: PointerButton::Left,
            },
        );
        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerUp {
                point: Point::new(10, 10),
                button: PointerButton::Left,
            },
        );

        let action = output
            .default_actions
            .iter()
            .find(|pending| pending.action.id().as_str() == "component.choose")
            .expect("component click action")
            .clone();
        let output = runtime.handle_default_action(action);
        assert_eq!(output.action_events.len(), 1);
        assert_eq!(output.action_events[0].target, component_id);
        assert_eq!(
            output.action_events[0].action.id().as_str(),
            "component.change"
        );
        assert_eq!(
            output.action_events[0].action.payload_value(),
            Some("chosen")
        );
    }

    #[test]
    fn text_input_default_action_can_be_prevented_before_control_mutation() {
        let input_id = UiId::new("input");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                input_id.clone(),
                UiNodeKind::Custom("test-input"),
                UiRect::new(0, 0, 100, 40),
            )
            .interaction(InteractionRole::Custom("input"))
            .on_event(super::super::UiEventKind::Input, |context, _| {
                context.prevent_default();
            }),
        );
        let mut runtime = UiRuntime::new();
        runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                point: Point::new(10, 10),
                button: PointerButton::Left,
            },
        );

        let output = runtime.handle_input(&tree, InputEvent::TextInput("你".to_owned()));

        assert_eq!(output.default_actions.len(), 1);
        assert_eq!(output.handler_events.len(), 1);
        assert_eq!(output.handler_events[0].target, input_id);
        assert_eq!(output.default_actions[0].action.payload_value(), Some("你"));
    }

    #[test]
    fn change_event_is_emitted_only_when_the_default_action_changes_state() {
        let input_id = UiId::new("input-change");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                input_id.clone(),
                UiNodeKind::Custom("test-input"),
                UiRect::new(0, 0, 100, 40),
            )
            .on_event(super::super::UiEventKind::Change, |_, _| {}),
        );
        let mut runtime = UiRuntime::new();
        runtime
            .component_states()
            .with_mut(&input_id, |_state: &mut InputActionState| {});
        runtime.reconcile_tree(&tree);
        let action = UiDefaultAction {
            event_target: input_id.clone(),
            action_target: input_id,
            action: UiAction::new("text.input").payload("value"),
        };

        let changed = runtime.handle_default_action(action.clone());
        let unchanged = runtime.handle_default_action(action);

        assert_eq!(changed.handler_events.len(), 1);
        assert!(unchanged.handler_events.is_empty());
    }

    #[test]
    fn ime_lifecycle_and_commit_use_the_generic_focused_event_route() {
        let input_id = UiId::new("ime-input");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                input_id.clone(),
                UiNodeKind::Custom("test-input"),
                UiRect::new(0, 0, 100, 40),
            )
            .interaction(InteractionRole::Custom("input"))
            .on_event(super::super::UiEventKind::CompositionStart, |_, _| {})
            .on_event(super::super::UiEventKind::CompositionUpdate, |_, _| {})
            .on_event(super::super::UiEventKind::CompositionEnd, |_, _| {})
            .on_event(super::super::UiEventKind::Input, |_, _| {}),
        );
        let mut runtime = UiRuntime::new();
        runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                point: Point::new(10, 10),
                button: PointerButton::Left,
            },
        );

        let start = runtime.handle_input(&tree, InputEvent::ImeStart);
        let update = runtime.handle_input(&tree, InputEvent::ImeUpdate("nǐ".to_owned()));
        let commit = runtime.handle_input(&tree, InputEvent::ImeCommit("你".to_owned()));
        let end = runtime.handle_input(&tree, InputEvent::ImeEnd);

        assert_eq!(start.handler_events[0].target, input_id);
        assert!(matches!(
            update.handler_events[0].payload,
            super::super::UiEventPayload::CompositionUpdate { ref text } if text == "nǐ"
        ));
        assert_eq!(commit.default_actions.len(), 1);
        assert!(matches!(
            commit.handler_events[0].payload,
            super::super::UiEventPayload::Input { ref text } if text == "你"
        ));
        assert_eq!(end.handler_events[0].target, input_id);
    }

    #[derive(Clone, Default)]
    struct PointerActionState {
        points: Vec<(i32, i32)>,
    }

    #[derive(Clone, Default)]
    struct SemanticActionState;

    #[derive(Clone, Default)]
    struct InputActionState {
        value: String,
    }

    impl ComponentState for SemanticActionState {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
            if action.id().as_str() != "component.choose" {
                return ComponentActionOutcome::ignored();
            }
            ComponentActionOutcome::handled(true)
                .emit(UiAction::new("component.change").payload("chosen"))
        }
    }

    impl ComponentState for InputActionState {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
            if action.id().as_str() != "text.input" {
                return ComponentActionOutcome::ignored();
            }
            let next = action.payload_value().unwrap_or_default();
            if self.value == next {
                return ComponentActionOutcome::handled(false);
            }
            self.value.clear();
            self.value.push_str(next);
            ComponentActionOutcome::handled(true)
        }
    }

    impl ComponentState for PointerActionState {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn handle_action(&mut self, action: &UiAction) -> ComponentActionOutcome {
            if !matches!(
                action.id().as_str(),
                POINTER_DOWN_ACTION | POINTER_DRAG_ACTION | POINTER_UP_ACTION
            ) {
                return ComponentActionOutcome::ignored();
            }
            let Some((x, y)) = action
                .payload_value()
                .and_then(|payload| payload.split_once(','))
                .and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
            else {
                return ComponentActionOutcome::ignored();
            };
            self.points.push((x, y));
            ComponentActionOutcome::handled(true)
        }
    }

    fn interactive_button_tree(id: UiId) -> HostTree {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(id, UiNodeKind::Button, UiRect::new(0, 0, 100, 40))
                .interaction(InteractionRole::Button)
                .animation(AnimationBinding::new(AnimProperty::Hover, 0.0, 1.0)),
        );
        tree
    }
}
