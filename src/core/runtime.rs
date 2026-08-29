use super::{
    apply_events_to_animations, component_state::RetainedNodeUpdate, AnimProperty,
    AnimationRegistry, ComponentStateStore, ComponentTree, ContextRegistry, DirtyTracker,
    EffectRegistry, HookStateStore, HostTree, InputEvent, KeyState, LogicalKey, NamedKey, UiAction,
    UiActionEvent, UiEvent, UiEventDispatcher, UiEventPayload, UiHandlerEvent, UiId, UiRect,
    UiTaskSpawner, UiUpdateQueue, UiWake,
};
use std::collections::HashSet;
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
    frame_interval_ms: Option<u64>,
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
        self.frame_interval_ms
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
        let focus_changed = self
            .hook_updates
            .take_focus_request()
            .is_some_and(|target| self.focus_node(tree, &target));
        if focus_changed {
            // The caller schedules focus damage from PendingUpdateOutput. Do not leak the same
            // dirty event into the next unrelated native input pass.
            let _ = self.dirty.take();
        }
        let frame_requested = self.hook_updates.take_frame_request();
        PendingUpdateOutput {
            dirty_ids: dirty,
            focus_changed,
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

    pub fn handle_input(&mut self, tree: &HostTree, input: InputEvent) -> RuntimeOutput {
        let focus_traversal = match &input {
            InputEvent::Keyboard(event)
                if event.state == KeyState::Down
                    && event.key == LogicalKey::Named(NamedKey::Tab) =>
            {
                Some(event.modifiers.shift())
            }
            _ => None,
        };
        let mut events = self.events.dispatch(tree, input);
        let action_events = Vec::new();
        let mut default_actions = Vec::new();
        events.retain(|event| {
            match event {
                UiEvent::Wheel { hit, delta } => {
                    let Some(action) = hit.action.as_ref() else {
                        return true;
                    };
                    let action = action.clone().payload(delta.y.to_string());
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
                UiEvent::SemanticValue { target, value } => {
                    default_actions.push(UiDefaultAction {
                        event_target: target.clone(),
                        action_target: target.clone(),
                        action: UiAction::new("semantic.set_value").payload(value.clone()),
                    });
                }
                UiEvent::SemanticAction { target, action } => {
                    let id = match action {
                        super::SemanticAction::Increment => "semantic.increment",
                        super::SemanticAction::Decrement => "semantic.decrement",
                        super::SemanticAction::ScrollIntoView => "semantic.scroll_into_view",
                        super::SemanticAction::ScrollUp => "semantic.scroll_up",
                        super::SemanticAction::ScrollDown => "semantic.scroll_down",
                        super::SemanticAction::ScrollLeft => "semantic.scroll_left",
                        super::SemanticAction::ScrollRight => "semantic.scroll_right",
                        super::SemanticAction::SetTextSelection => "semantic.set_text_selection",
                        super::SemanticAction::Click
                        | super::SemanticAction::Focus
                        | super::SemanticAction::Blur
                        | super::SemanticAction::SetValue => return true,
                    };
                    default_actions.push(UiDefaultAction {
                        event_target: target.clone(),
                        action_target: target.clone(),
                        action: UiAction::new(id),
                    });
                }
                UiEvent::Keyboard { target, event } if event.state == KeyState::Down => {
                    let action = match &event.key {
                        LogicalKey::Named(NamedKey::Backspace) => {
                            Some(UiAction::new("text.backspace"))
                        }
                        LogicalKey::Named(NamedKey::ArrowLeft) => Some(
                            UiAction::new("text.move.left").payload(if event.modifiers.shift() {
                                "extend"
                            } else {
                                "collapse"
                            }),
                        ),
                        LogicalKey::Named(NamedKey::ArrowRight) => Some(
                            UiAction::new("text.move.right").payload(if event.modifiers.shift() {
                                "extend"
                            } else {
                                "collapse"
                            }),
                        ),
                        LogicalKey::Named(NamedKey::ArrowUp) => Some(
                            UiAction::new("text.move.up").payload(if event.modifiers.shift() {
                                "extend"
                            } else {
                                "collapse"
                            }),
                        ),
                        LogicalKey::Named(NamedKey::ArrowDown) => Some(
                            UiAction::new("text.move.down").payload(if event.modifiers.shift() {
                                "extend"
                            } else {
                                "collapse"
                            }),
                        ),
                        LogicalKey::Named(NamedKey::Enter) => {
                            Some(UiAction::new("text.input").payload("\n"))
                        }
                        LogicalKey::Character(key)
                            if event.modifiers.ctrl() && key.eq_ignore_ascii_case("a") =>
                        {
                            Some(UiAction::new("text.select.all"))
                        }
                        LogicalKey::Character(key)
                            if event.modifiers.ctrl() && key.eq_ignore_ascii_case("c") =>
                        {
                            Some(UiAction::new("text.copy"))
                        }
                        LogicalKey::Character(key)
                            if event.modifiers.ctrl() && key.eq_ignore_ascii_case("v") =>
                        {
                            Some(UiAction::new("text.paste"))
                        }
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
                UiEvent::Keyboard { .. } => {}
                UiEvent::PointerPressed { hit, pointer } => {
                    let point = pointer.point;
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
                UiEvent::PointerDragged { hit, pointer } => {
                    let point = pointer.point;
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
                UiEvent::PointerReleased { hit, pointer } => {
                    let point = pointer.point;
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
        // Components may derive visuals directly from `interaction_flags` without declaring an
        // animation. Hover, press and focus changes therefore invalidate their component owners
        // independently from animation target updates. Raw pointer coordinates remain excluded.
        self.mark_component_owners(tree, events.iter().flat_map(interaction_state_target_ids));
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

    pub fn advance(&mut self, tree: &mut HostTree, elapsed_ms: f32) -> RuntimeOutput {
        let animation_changed = self.animations.advance(elapsed_ms);
        let mut frame_interval_ms = animation_changed.then_some(16);
        let animation_ids = self.animations.take_dirty_ids();
        self.mark_component_owners(tree, animation_ids.iter());
        self.dirty.mark_animation_ids(animation_ids);
        let component_invalidations = self.component_states.advance_invalidations(elapsed_ms);
        let component_changed = !component_invalidations.is_empty();
        let mut route_changed = false;
        let mut retained_dirty_bounds: Option<UiRect> = None;
        let mut regular_dirty_ids = Vec::new();
        for invalidation in &component_invalidations {
            frame_interval_ms = Some(
                frame_interval_ms.map_or(invalidation.frame_interval_ms, |current| {
                    current.min(invalidation.frame_interval_ms)
                }),
            );
            if let Some(RetainedNodeUpdate::CompositingLayer(spec)) = invalidation.retained_update {
                if let Some(bounds) = tree.update_compositing_layer(&invalidation.target_id, spec) {
                    retained_dirty_bounds =
                        Some(retained_dirty_bounds.map_or(bounds, |current| current.union(bounds)));
                }
            } else if let Some(owner) = invalidation.owner {
                self.component_tree.mark_dirty(owner);
                regular_dirty_ids.push(invalidation.target_id.clone());
            } else {
                self.mark_component_owners(tree, std::iter::once(&invalidation.target_id));
                regular_dirty_ids.push(invalidation.target_id.clone());
            }
            route_changed |= self
                .component_states
                .take_route_invalidation(&invalidation.state_id);
        }
        self.dirty.mark_animation_ids(regular_dirty_ids);
        let dirty_bounds = match (self.dirty.take().bounds(tree), retained_dirty_bounds) {
            (Some(regular), Some(retained)) => Some(regular.union(retained)),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => None,
        };
        self.frame_interval_ms = frame_interval_ms;
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

    pub fn handle_default_action(
        &mut self,
        tree: &HostTree,
        pending: UiDefaultAction,
    ) -> RuntimeOutput {
        if pending.action.id().as_str() == FOCUS_TRAVERSAL_ACTION {
            let events = self
                .events
                .focus_adjacent(tree, pending.action.payload_value() == Some("reverse"));
            let handler_events = events
                .iter()
                .flat_map(|event| tree.handler_events(event))
                .collect();
            for event in events.iter().cloned() {
                self.dirty.mark_event(event);
            }
            self.mark_component_owners(tree, events.iter().flat_map(interaction_state_target_ids));
            let animation_changed =
                apply_events_to_animations(tree, &mut self.animations, events.iter().cloned());
            if animation_changed {
                self.mark_component_owners(tree, events.iter().flat_map(event_target_ids));
            }
            let dirty_bounds = self.dirty.take().bounds(tree);
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
            tree,
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
        let dirty_bounds = self.dirty.take().bounds(tree);
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
            self.mark_component_owners(tree, std::iter::once(target));
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
        changed |= self.animations.clear_absent_values_by(
            |id| tree.node(id).is_some(),
            &[
                AnimProperty::Hover,
                AnimProperty::Active,
                AnimProperty::Pressed,
                AnimProperty::Focus,
            ],
        );
        let sync_ids = tree.animation_sync_ids().cloned().collect::<Vec<_>>();
        for node in sync_ids.iter().filter_map(|id| tree.node(id)) {
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
        if !tree.needs_focus_sync() {
            return false;
        }
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

    pub fn focus_node(&mut self, tree: &HostTree, id: &UiId) -> bool {
        let events = self.events.focus_node(tree, id);
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
        | UiEvent::Keyboard { target, .. }
        | UiEvent::SemanticValue { target, .. }
        | UiEvent::SemanticAction { target, .. } => vec![target],
        UiEvent::FocusChanged { current, previous } => previous
            .iter()
            .chain(current.iter().map(|hit| &hit.id))
            .collect(),
        UiEvent::PointerLeft { previous } => previous.iter().collect(),
    }
}

fn interaction_state_target_ids(event: &UiEvent) -> Vec<&UiId> {
    match event {
        UiEvent::HoverChanged { previous, current }
        | UiEvent::PressedChanged { previous, current } => previous
            .iter()
            .chain(current.iter().map(|hit| &hit.id))
            .collect(),
        UiEvent::FocusChanged { previous, current } => previous
            .iter()
            .chain(current.iter().map(|hit| &hit.id))
            .collect(),
        UiEvent::PointerLeft { previous } => previous.iter().collect(),
        UiEvent::Clicked(_)
        | UiEvent::Wheel { .. }
        | UiEvent::TextInput { .. }
        | UiEvent::ImeStarted { .. }
        | UiEvent::ImeUpdated { .. }
        | UiEvent::ImeEnded { .. }
        | UiEvent::Keyboard { .. }
        | UiEvent::PointerPressed { .. }
        | UiEvent::PointerMoved { .. }
        | UiEvent::PointerDragged { .. }
        | UiEvent::PointerReleased { .. } => Vec::new(),
        UiEvent::SemanticValue { .. } | UiEvent::SemanticAction { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::super::{
        compile_scene, AnimationBinding, ComponentActionOutcome, ComponentState,
        CompositingLayerAnimation, CompositingLayerSpec, ImeEvent, InputEvent, InteractionRole,
        KeyModifiers, KeyboardEvent, Point, PointerButton, PointerData, ScenePrimitive, UiNode,
        UiNodeKind, VisualStyle, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION, POINTER_UP_ACTION,
    };
    use super::*;

    fn key_down(key: NamedKey, modifiers: KeyModifiers) -> InputEvent {
        InputEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: LogicalKey::Named(key),
            modifiers,
            ..Default::default()
        })
    }

    #[derive(Clone, Default)]
    struct RetainedLayerAnimation {
        translation_x: f32,
    }

    impl CompositingLayerAnimation for RetainedLayerAnimation {
        fn advance(&mut self, _elapsed_ms: f32) -> bool {
            self.translation_x += 10.0;
            true
        }

        fn compositing_layer_spec(&self) -> CompositingLayerSpec {
            CompositingLayerSpec::new().translation(self.translation_x, 0.0)
        }
    }

    fn compositing_content_signature(scene: &super::super::Scene) -> u64 {
        scene
            .commands()
            .iter()
            .find_map(|command| match command {
                ScenePrimitive::CompositingLayer {
                    content_signature, ..
                } => Some(*content_signature),
                _ => None,
            })
            .expect("compositing layer command")
    }

    #[test]
    fn tree_sync_drops_hover_for_removed_nodes() {
        let button_id = UiId::new("start");
        let mut button_tree = interactive_button_tree(button_id.clone());
        let empty_tree = HostTree::new();
        let mut runtime = UiRuntime::new();

        runtime.handle_input(
            &button_tree,
            InputEvent::PointerMove(PointerData::mouse(Point::new(10.0, 10.0))),
        );
        runtime.advance(&mut button_tree, 1000.0);

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
    fn pointer_motion_inside_the_same_hover_target_does_not_request_paint() {
        let button_tree = interactive_button_tree(UiId::new("steady-hover"));
        let mut runtime = UiRuntime::new();

        let entered = runtime.handle_input(
            &button_tree,
            InputEvent::PointerMove(PointerData::mouse(Point::new(10.0, 10.0))),
        );
        assert!(entered.dirty_bounds.is_some());

        let moved = runtime.handle_input(
            &button_tree,
            InputEvent::PointerMove(PointerData::mouse(Point::new(11.0, 10.0))),
        );

        assert!(moved
            .events
            .iter()
            .any(|event| matches!(event, UiEvent::PointerMoved { .. })));
        assert_eq!(moved.dirty_bounds, None);
        assert!(!moved.animation_changed);
    }

    #[test]
    fn tree_sync_drops_active_animation_for_removed_nodes() {
        let switch_id = UiId::new("switch");
        let mut switch_tree = HostTree::new();
        switch_tree.push(
            UiNode::new(
                switch_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 46.0, 24.0),
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
                UiRect::new(0.0, 0.0, 20.0, 20.0),
            )
            .component_owner(owner)
            .animation(AnimationBinding::new(AnimProperty::Active, 0.0, 1.0))
            .animation_target(AnimProperty::Active, true),
        );

        assert!(runtime.sync_tree_animation_targets(&tree));
        assert!(runtime.component_tree().is_dirty(owner));
    }

    #[test]
    fn bound_component_state_animation_invalidates_only_its_host_node() {
        let mut runtime = UiRuntime::new();
        let (owner, unrelated_owner) = {
            let components = runtime.component_tree();
            components.begin_render();
            let owner = components.root(UiId::owned("rail-owner"), "rail-owner");
            components.begin_component_execution(owner);
            components.finish_component(owner);
            let unrelated_owner =
                components.root(UiId::owned("unrelated-owner"), "unrelated-owner");
            components.begin_component_execution(unrelated_owner);
            components.finish_component(unrelated_owner);
            components.end_render();
            (owner, unrelated_owner)
        };
        let state_id = UiId::owned("rail.h.state.0");
        let target_id = UiId::owned("rail");
        let target_bounds = UiRect::new(10.0, 20.0, 24.0, 220.0);
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(target_id.clone(), UiNodeKind::Panel, target_bounds).component_owner(owner),
        );
        runtime.component_states().with_mut_for_component(
            &state_id,
            owner,
            target_id,
            |_state: &mut AlwaysAnimatingState| {},
        );

        let output = runtime.advance(&mut tree, 16.0);

        assert!(output.animation_changed);
        assert_eq!(output.dirty_bounds, Some(target_bounds));
        assert_eq!(runtime.frame_interval_ms(), Some(33));
        assert!(runtime.component_tree().is_dirty(owner));
        assert!(!runtime.component_tree().is_dirty(unrelated_owner));
    }

    #[test]
    fn retained_layer_animation_updates_composition_without_dirtying_component() {
        let mut runtime = UiRuntime::new();
        let owner = {
            let components = runtime.component_tree();
            components.begin_render();
            let owner = components.root(UiId::owned("retained-owner"), "retained-owner");
            components.begin_component_execution(owner);
            components.finish_component(owner);
            components.end_render();
            owner
        };
        let layer_id = UiId::owned("animated-layer");
        let child_id = UiId::owned("static-child");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                layer_id.clone(),
                UiNodeKind::CompositingLayer,
                UiRect::new(0.0, 0.0, 20.0, 20.0),
            )
            .component_owner(owner)
            .compositing_layer(CompositingLayerSpec::new()),
        );
        tree.push(
            UiNode::new(
                child_id,
                UiNodeKind::Panel,
                UiRect::new(2.0, 2.0, 18.0, 18.0),
            )
            .parent(layer_id.clone())
            .component_owner(owner)
            .style(VisualStyle::filled(super::super::Color::WHITE)),
        );
        let _ = tree.take_projection_changes();
        let before_signature = compositing_content_signature(&compile_scene(&tree));
        runtime.component_states().with_mut_for_compositing_layer(
            &layer_id,
            layer_id.clone(),
            |_state: &mut RetainedLayerAnimation| {},
        );

        let output = runtime.advance(&mut tree, 16.0);

        assert!(output.animation_changed);
        assert_eq!(output.dirty_bounds, Some(UiRect::new(0.0, 0.0, 30.0, 20.0)));
        assert!(!runtime.component_tree().is_dirty(owner));
        assert_eq!(
            tree.node(&layer_id)
                .and_then(|node| node.compositing_layer)
                .expect("layer spec")
                .transform
                .translation_x(),
            10.0
        );
        let changes = tree.take_projection_changes();
        assert_eq!(changes.changed.len(), 1);
        assert!(changes.changed.contains(&layer_id));
        assert!(!changes.structure_changed);
        assert_eq!(
            compositing_content_signature(&compile_scene(&tree)),
            before_signature
        );
    }

    #[test]
    fn faster_core_animation_wins_over_component_frame_interval() {
        let mut runtime = UiRuntime::new();
        let owner = {
            let components = runtime.component_tree();
            components.begin_render();
            let owner = components.root(UiId::owned("mixed-owner"), "mixed-owner");
            components.begin_component_execution(owner);
            components.finish_component(owner);
            components.end_render();
            owner
        };
        let rail_id = UiId::owned("mixed-rail");
        let animation_id = UiId::owned("mixed-animation");
        let animation = AnimationBinding::new(AnimProperty::Active, 0.0, 1.0);
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                rail_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 14.0, 200.0),
            )
            .component_owner(owner),
        );
        tree.push(
            UiNode::new(
                animation_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(20.0, 0.0, 40.0, 20.0),
            )
            .component_owner(owner)
            .animation(animation)
            .animation_target(AnimProperty::Active, true),
        );
        assert!(runtime
            .animations_mut()
            .set_binding_target(animation_id, animation, true));
        runtime.component_states().with_mut_for_component(
            &UiId::owned("mixed-rail.h.state.0"),
            owner,
            rail_id,
            |_state: &mut AlwaysAnimatingState| {},
        );

        let output = runtime.advance(&mut tree, 16.0);

        assert!(output.animation_changed);
        assert_eq!(runtime.frame_interval_ms(), Some(16));
    }

    #[test]
    fn pointer_down_on_empty_space_clears_focus() {
        let button_id = UiId::new("focus-target");
        let tree = interactive_button_tree(button_id.clone());
        let mut runtime = UiRuntime::new();

        runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
                button: PointerButton::Left,
            },
        );
        assert_eq!(runtime.interaction_state().focused, Some(button_id));

        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                pointer: PointerData::mouse(Point::new(200.0, 200.0)),
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
                UiRect::new(0.0, 0.0, 40.0, 20.0),
            )
            .interaction(InteractionRole::Button),
        );
        let mut modal_tree = base_tree.clone();
        modal_tree.push(
            UiNode::new(
                scope.clone(),
                UiNodeKind::Group,
                UiRect::new(0.0, 0.0, 100.0, 100.0),
            )
            .focus_scope(true),
        );
        for (id, rect) in [
            (first.clone(), UiRect::new(10.0, 10.0, 40.0, 30.0)),
            (second.clone(), UiRect::new(50.0, 10.0, 80.0, 30.0)),
        ] {
            modal_tree.push(
                UiNode::new(id, UiNodeKind::Button, rect)
                    .parent(scope.clone())
                    .interaction(InteractionRole::Button),
            );
        }
        let mut runtime = UiRuntime::new();
        assert!(runtime.focus_node(&base_tree, &background));

        assert!(runtime.sync_tree_focus(&modal_tree));
        assert_eq!(runtime.interaction_state().focused, Some(first.clone()));

        let output = runtime.handle_input(
            &modal_tree,
            key_down(NamedKey::Tab, KeyModifiers::default()),
        );
        assert_eq!(runtime.interaction_state().focused, Some(first));
        assert_eq!(output.default_actions.len(), 1);
        runtime.handle_default_action(&modal_tree, output.default_actions[0].clone());
        assert_eq!(runtime.interaction_state().focused, Some(second));

        assert!(runtime.sync_tree_focus(&base_tree));
        assert_eq!(runtime.interaction_state().focused, Some(background));
    }

    #[test]
    fn tab_focus_traversal_invalidates_retained_focus_visuals() {
        let first = UiId::owned("first");
        let second = UiId::owned("second");
        let mut runtime = UiRuntime::new();
        let owner = {
            let components = runtime.component_tree();
            components.begin_render();
            let owner = components.root(UiId::owned("focus-owner"), "focus-owner");
            components.begin_component_execution(owner);
            components.finish_component(owner);
            components.end_render();
            owner
        };

        let mut tree = HostTree::new();
        for (id, rect) in [
            (first.clone(), UiRect::new(0.0, 0.0, 40.0, 20.0)),
            (second.clone(), UiRect::new(50.0, 0.0, 90.0, 20.0)),
        ] {
            tree.push(
                UiNode::new(id, UiNodeKind::Button, rect)
                    .component_owner(owner)
                    .interaction(InteractionRole::Button),
            );
        }
        assert!(runtime.focus_node(&tree, &first));

        {
            let components = runtime.component_tree();
            components.begin_render();
            let retained_owner = components.root(UiId::owned("focus-owner"), "focus-owner");
            assert_eq!(retained_owner, owner);
            components.begin_component_execution(retained_owner);
            components.finish_component(retained_owner);
            components.end_render();
            assert!(!components.is_dirty(owner));
        }

        let output = runtime.handle_input(&tree, key_down(NamedKey::Tab, KeyModifiers::default()));
        runtime.handle_default_action(&tree, output.default_actions[0].clone());

        assert_eq!(runtime.interaction_state().focused, Some(second));
        assert!(runtime.component_tree().is_dirty(owner));
    }

    #[test]
    fn tab_focus_traversal_is_deferred_until_after_key_handlers() {
        let id = UiId::owned("focusable");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                id.clone(),
                UiNodeKind::Button,
                UiRect::new(0.0, 0.0, 40.0, 20.0),
            )
            .interaction(InteractionRole::Button)
            .on_event(super::super::UiEventKind::KeyDown, |context, _| {
                context.prevent_default();
            }),
        );
        let mut runtime = UiRuntime::new();
        assert!(runtime.focus_node(&tree, &id));

        let output = runtime.handle_input(&tree, key_down(NamedKey::Tab, KeyModifiers::default()));

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
        runtime.hook_updates().request_focus(target.clone());
        let output = runtime.apply_pending_updates(&tree);

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
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
                button: PointerButton::Left,
            },
        );
        for action in output.default_actions {
            runtime.handle_default_action(&tree, action);
        }
        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerMove(PointerData::mouse(Point::new(180.0, 10.0))),
        );
        for action in output.default_actions {
            runtime.handle_default_action(&tree, action);
        }
        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerUp {
                pointer: PointerData::mouse(Point::new(180.0, 10.0)),
                button: PointerButton::Left,
            },
        );
        for action in output.default_actions {
            runtime.handle_default_action(&tree, action);
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
                UiRect::new(0.0, 0.0, 100.0, 40.0),
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
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
                button: PointerButton::Left,
            },
        );
        let output = runtime.handle_input(
            &tree,
            InputEvent::PointerUp {
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
                button: PointerButton::Left,
            },
        );

        let action = output
            .default_actions
            .iter()
            .find(|pending| pending.action.id().as_str() == "component.choose")
            .expect("component click action")
            .clone();
        let output = runtime.handle_default_action(&tree, action);
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
    fn changed_component_actions_dirty_only_the_owning_component() {
        let target = UiId::owned("local-state-target");
        let mut runtime = UiRuntime::new();
        let components = runtime.component_tree();
        components.begin_render();
        let owner = components.root(UiId::owned("local-state-owner"), "local-state-owner");
        components.begin_component_execution(owner);
        components.finish_component(owner);
        components.end_render();
        assert!(!components.is_dirty(owner));

        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                target.clone(),
                UiNodeKind::Button,
                UiRect::new(20.0, 30.0, 120.0, 70.0),
            )
            .component_owner(owner),
        );
        runtime
            .component_states()
            .with_mut(&target, |_state: &mut SemanticActionState| {});

        let output = runtime.handle_default_action(
            &tree,
            UiDefaultAction {
                event_target: target.clone(),
                action_target: target,
                action: UiAction::new("component.choose"),
            },
        );

        assert!(output.animation_changed);
        assert_eq!(
            output.dirty_bounds,
            Some(UiRect::new(20.0, 30.0, 120.0, 70.0))
        );
        assert!(runtime.component_tree().is_dirty(owner));
    }

    #[test]
    fn text_input_default_action_can_be_prevented_before_control_mutation() {
        let input_id = UiId::new("input");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                input_id.clone(),
                UiNodeKind::Custom("test-input"),
                UiRect::new(0.0, 0.0, 100.0, 40.0),
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
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
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
                UiRect::new(0.0, 0.0, 100.0, 40.0),
            )
            .on_event(super::super::UiEventKind::Change, |_, _| {}),
        );
        let mut runtime = UiRuntime::new();
        runtime
            .component_states()
            .with_mut(&input_id, |_state: &mut InputActionState| {});
        let action = UiDefaultAction {
            event_target: input_id.clone(),
            action_target: input_id,
            action: UiAction::new("text.input").payload("value"),
        };

        let changed = runtime.handle_default_action(&tree, action.clone());
        let unchanged = runtime.handle_default_action(&tree, action);

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
                UiRect::new(0.0, 0.0, 100.0, 40.0),
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
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
                button: PointerButton::Left,
            },
        );

        let start = runtime.handle_input(&tree, InputEvent::Ime(ImeEvent::Enabled));
        let update = runtime.handle_input(
            &tree,
            InputEvent::Ime(ImeEvent::Preedit {
                text: "nǐ".to_owned(),
                cursor: Some(1..1),
            }),
        );
        let commit =
            runtime.handle_input(&tree, InputEvent::Ime(ImeEvent::Commit("中文".to_owned())));
        let end = runtime.handle_input(&tree, InputEvent::Ime(ImeEvent::Disabled));

        assert_eq!(start.handler_events[0].target, input_id);
        assert!(matches!(
            update.handler_events[0].payload,
            super::super::UiEventPayload::CompositionUpdate { ref text, ref cursor }
                if text == "nǐ" && cursor == &Some(1..1)
        ));
        assert_eq!(commit.default_actions.len(), 1);
        assert!(matches!(
            commit.handler_events[0].payload,
            super::super::UiEventPayload::Input { ref text } if text == "中文"
        ));
        assert_eq!(
            commit.default_actions[0].action.payload_value(),
            Some("中文")
        );
        assert_eq!(end.handler_events[0].target, input_id);
    }

    #[test]
    fn backspace_uses_the_focused_text_default_action_route() {
        let input_id = UiId::new("backspace-input");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                input_id.clone(),
                UiNodeKind::Custom("test-input"),
                UiRect::new(0.0, 0.0, 100.0, 40.0),
            )
            .interaction(InteractionRole::Custom("input")),
        );
        let mut runtime = UiRuntime::new();
        runtime.handle_input(
            &tree,
            InputEvent::PointerDown {
                pointer: PointerData::mouse(Point::new(10.0, 10.0)),
                button: PointerButton::Left,
            },
        );

        let output = runtime.handle_input(
            &tree,
            key_down(NamedKey::Backspace, KeyModifiers::default()),
        );

        assert_eq!(output.default_actions.len(), 1);
        assert_eq!(output.default_actions[0].event_target, input_id);
        assert_eq!(
            output.default_actions[0].action.id().as_str(),
            "text.backspace"
        );
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

    #[derive(Clone, Default)]
    struct AlwaysAnimatingState;

    impl ComponentState for AlwaysAnimatingState {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn advance(&mut self, _elapsed_ms: f32) -> bool {
            true
        }

        fn frame_interval_ms(&self) -> u64 {
            33
        }
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
            UiNode::new(id, UiNodeKind::Button, UiRect::new(0.0, 0.0, 100.0, 40.0))
                .interaction(InteractionRole::Button)
                .animation(AnimationBinding::new(AnimProperty::Hover, 0.0, 1.0)),
        );
        tree
    }
}
