use super::*;

impl UiRuntime {
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
}

pub(super) fn event_target_ids(event: &UiEvent) -> Vec<&UiId> {
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

pub(super) fn interaction_state_target_ids(event: &UiEvent) -> Vec<&UiId> {
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
