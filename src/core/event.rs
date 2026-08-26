use std::collections::HashMap;

use super::{AnimProperty, AnimationRegistry, HitResult, HostTree, Point, UiId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Tab,
    Backspace,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Enter,
    A,
    C,
    V,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    PointerMove(Point),
    PointerDown {
        point: Point,
        button: PointerButton,
    },
    PointerUp {
        point: Point,
        button: PointerButton,
    },
    Wheel {
        point: Point,
        delta_y: i32,
    },
    TextInput(String),
    ImeStart,
    ImeUpdate(String),
    ImeCommit(String),
    ImeEnd,
    Backspace,
    KeyDown {
        key: KeyCode,
        modifiers: KeyModifiers,
    },
    PointerLeave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    HoverChanged {
        previous: Option<UiId>,
        current: Option<HitResult>,
    },
    PressedChanged {
        previous: Option<UiId>,
        current: Option<HitResult>,
    },
    Clicked(HitResult),
    Wheel {
        hit: HitResult,
        delta_y: i32,
    },
    TextInput {
        target: UiId,
        text: String,
    },
    ImeStarted {
        target: UiId,
    },
    ImeUpdated {
        target: UiId,
        text: String,
    },
    ImeEnded {
        target: UiId,
    },
    Backspace {
        target: UiId,
    },
    KeyDown {
        target: UiId,
        key: KeyCode,
        modifiers: KeyModifiers,
    },
    PointerPressed {
        hit: HitResult,
        point: Point,
    },
    PointerMoved {
        hit: HitResult,
        point: Point,
    },
    PointerDragged {
        hit: HitResult,
        point: Point,
    },
    PointerReleased {
        hit: HitResult,
        point: Point,
    },
    FocusChanged {
        previous: Option<UiId>,
        current: Option<HitResult>,
    },
    PointerLeft {
        previous: Option<UiId>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiInteractionState {
    pub hovered: Option<UiId>,
    pub pressed: Option<UiId>,
    pub focused: Option<UiId>,
}

impl UiInteractionState {
    pub fn flags_for(&self, id: &UiId) -> InteractionFlags {
        InteractionFlags {
            hovered: self.hovered.as_ref() == Some(id),
            pressed: self.pressed.as_ref() == Some(id),
            focused: self.focused.as_ref() == Some(id),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InteractionFlags {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

pub struct UiEventDispatcher {
    state: UiInteractionState,
    pressed_hit: Option<HitResult>,
    active_focus_scope: Option<UiId>,
    focus_history: HashMap<UiId, Option<UiId>>,
}

impl UiEventDispatcher {
    pub fn new() -> Self {
        Self {
            state: UiInteractionState::default(),
            pressed_hit: None,
            active_focus_scope: None,
            focus_history: HashMap::new(),
        }
    }

    pub fn state(&self) -> UiInteractionState {
        self.state.clone()
    }

    pub fn clear_interaction_state(&mut self) -> bool {
        if self.state == UiInteractionState::default() {
            return false;
        }
        self.state = UiInteractionState::default();
        self.active_focus_scope = None;
        self.focus_history.clear();
        true
    }

    pub fn dispatch(&mut self, tree: &HostTree, event: InputEvent) -> Vec<UiEvent> {
        match event {
            InputEvent::PointerMove(point) => self.pointer_move(tree, point),
            InputEvent::PointerDown { point, .. } => self.pointer_down(tree, point),
            InputEvent::PointerUp { point, .. } => self.pointer_up(tree, point),
            InputEvent::Wheel { point, delta_y } => self.wheel(tree, point, delta_y),
            InputEvent::TextInput(text) | InputEvent::ImeCommit(text) => self.text_input(text),
            InputEvent::ImeStart => self.ime_start(),
            InputEvent::ImeUpdate(text) => self.ime_update(text),
            InputEvent::ImeEnd => self.ime_end(),
            InputEvent::Backspace => self.backspace(),
            InputEvent::KeyDown { key, modifiers } => self.key_down(tree, key, modifiers),
            InputEvent::PointerLeave => self.pointer_leave(),
        }
    }

    fn pointer_move(&mut self, tree: &HostTree, point: Point) -> Vec<UiEvent> {
        let hit = tree.hit_test(point).filter(|hit| hit.policy.hover);
        let current = hit.as_ref().map(|hit| hit.id.clone());
        let drag = self
            .pressed_hit
            .clone()
            .map(|hit| UiEvent::PointerDragged { hit, point });
        if self.state.hovered == current {
            let mut events = hit
                .map(|hit| UiEvent::PointerMoved { hit, point })
                .into_iter()
                .collect::<Vec<_>>();
            events.extend(drag);
            return events;
        }
        let previous = self.state.hovered.clone();
        self.state.hovered = current.clone();
        let mut events = vec![UiEvent::HoverChanged {
            previous,
            current: hit,
        }];
        if let Some(event) = drag {
            events.push(event);
        }
        if let Some(hit) = current.as_ref().and_then(|id| tree.hit_for_id(id)) {
            events.push(UiEvent::PointerMoved { hit, point });
        }
        events
    }

    fn pointer_down(&mut self, tree: &HostTree, point: Point) -> Vec<UiEvent> {
        let hit = tree.hit_test(point).filter(|hit| hit.policy.press);
        let current = hit.as_ref().map(|hit| hit.id.clone());
        let mut events = Vec::new();
        if self.state.pressed != current {
            let previous = self.state.pressed.clone();
            self.state.pressed = current.clone();
            self.pressed_hit = hit.clone();
            events.push(UiEvent::PressedChanged {
                previous,
                current: hit.clone(),
            });
        } else {
            self.pressed_hit = hit.clone();
        }
        if let Some(hit) = hit {
            if hit.policy.focus {
                let previous_focus = self.state.focused.clone();
                self.state.focused = Some(hit.id.clone());
                if previous_focus != self.state.focused {
                    events.push(UiEvent::FocusChanged {
                        previous: previous_focus,
                        current: Some(hit.clone()),
                    });
                }
            }
            events.push(UiEvent::PointerPressed { hit, point });
        } else if let Some(previous) = self.state.focused.take() {
            events.push(UiEvent::FocusChanged {
                previous: Some(previous),
                current: None,
            });
        }
        events
    }

    fn pointer_up(&mut self, tree: &HostTree, point: Point) -> Vec<UiEvent> {
        let hit = tree.hit_test(point).filter(|hit| hit.policy.press);
        let mut events = Vec::new();
        let previous = self.state.pressed.clone();
        let pressed_hit = self.pressed_hit.take();
        self.state.pressed = None;
        if previous.is_some() {
            events.push(UiEvent::PressedChanged {
                previous: previous.clone(),
                current: None,
            });
        }
        if let Some(hit) = pressed_hit {
            events.push(UiEvent::PointerReleased { hit, point });
        }
        if let Some(hit) = hit {
            if previous == Some(hit.id.clone()) {
                if hit.policy.focus {
                    let previous_focus = self.state.focused.clone();
                    self.state.focused = Some(hit.id.clone());
                    if previous_focus != self.state.focused {
                        events.push(UiEvent::FocusChanged {
                            previous: previous_focus,
                            current: Some(hit.clone()),
                        });
                    }
                }
                events.push(UiEvent::Clicked(hit));
            }
        }
        events
    }

    fn wheel(&mut self, tree: &HostTree, point: Point, delta_y: i32) -> Vec<UiEvent> {
        tree.wheel_hit_test(point)
            .map(|hit| vec![UiEvent::Wheel { hit, delta_y }])
            .unwrap_or_default()
    }

    fn text_input(&mut self, text: String) -> Vec<UiEvent> {
        self.state
            .focused
            .clone()
            .map(|target| vec![UiEvent::TextInput { target, text }])
            .unwrap_or_default()
    }

    fn ime_start(&self) -> Vec<UiEvent> {
        self.state
            .focused
            .clone()
            .map(|target| vec![UiEvent::ImeStarted { target }])
            .unwrap_or_default()
    }

    fn ime_update(&self, text: String) -> Vec<UiEvent> {
        self.state
            .focused
            .clone()
            .map(|target| vec![UiEvent::ImeUpdated { target, text }])
            .unwrap_or_default()
    }

    fn ime_end(&self) -> Vec<UiEvent> {
        self.state
            .focused
            .clone()
            .map(|target| vec![UiEvent::ImeEnded { target }])
            .unwrap_or_default()
    }

    fn backspace(&mut self) -> Vec<UiEvent> {
        self.state
            .focused
            .clone()
            .map(|target| vec![UiEvent::Backspace { target }])
            .unwrap_or_default()
    }

    fn key_down(
        &mut self,
        _tree: &HostTree,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> Vec<UiEvent> {
        self.state
            .focused
            .clone()
            .map(|target| {
                vec![UiEvent::KeyDown {
                    target,
                    key,
                    modifiers,
                }]
            })
            .unwrap_or_default()
    }

    pub(crate) fn focus_adjacent(&mut self, tree: &HostTree, reverse: bool) -> Vec<UiEvent> {
        let focusable = tree
            .active_focus_scope_id()
            .map(|scope_id| tree.focusable_hits_in_scope(&scope_id))
            .unwrap_or_else(|| tree.focusable_hits());
        if focusable.is_empty() {
            return Vec::new();
        }
        let current_index = self
            .state
            .focused
            .as_ref()
            .and_then(|focused| focusable.iter().position(|hit| &hit.id == focused));
        let next_index = match (current_index, reverse) {
            (Some(index), false) => (index + 1) % focusable.len(),
            (Some(0), true) => focusable.len() - 1,
            (Some(index), true) => index - 1,
            (None, false) => 0,
            (None, true) => focusable.len() - 1,
        };
        let next = focusable[next_index].clone();
        self.set_focus(Some(next))
    }

    pub fn sync_focus_for_tree(&mut self, tree: &HostTree) -> Vec<UiEvent> {
        let focus_scope_ids = tree.focus_scope_ids();
        let next_scope = tree.active_focus_scope_id();

        if self.active_focus_scope != next_scope {
            let previous_scope = self.active_focus_scope.take();
            let restored = previous_scope
                .as_ref()
                .filter(|scope_id| !focus_scope_ids.iter().any(|id| id == *scope_id))
                .and_then(|scope_id| self.focus_history.remove(scope_id))
                .flatten();
            self.focus_history
                .retain(|scope_id, _| focus_scope_ids.iter().any(|id| id == scope_id));

            if let Some(scope_id) = next_scope.clone() {
                self.focus_history
                    .entry(scope_id.clone())
                    .or_insert_with(|| self.state.focused.clone());
                self.active_focus_scope = Some(scope_id.clone());
                let target = restored
                    .as_ref()
                    .and_then(|id| tree.focusable_hit_in_scope(id, &scope_id))
                    .or_else(|| tree.auto_focus_hit_in_scope(&scope_id))
                    .or_else(|| tree.focusable_hits_in_scope(&scope_id).into_iter().next());
                return self.set_focus(target);
            }

            let target = restored
                .as_ref()
                .and_then(|id| tree.focusable_hit(id))
                .or_else(|| tree.auto_focus_hit());
            return self.set_focus(target);
        }

        self.focus_history
            .retain(|scope_id, _| focus_scope_ids.iter().any(|id| id == scope_id));

        if let Some(scope_id) = next_scope {
            if self
                .state
                .focused
                .as_ref()
                .is_some_and(|id| tree.focusable_hit_in_scope(id, &scope_id).is_some())
            {
                return Vec::new();
            }
            let target = tree
                .auto_focus_hit_in_scope(&scope_id)
                .or_else(|| tree.focusable_hits_in_scope(&scope_id).into_iter().next());
            return self.set_focus(target);
        }

        if let Some(focused) = self.state.focused.as_ref() {
            if tree.focusable_hit(focused).is_some() {
                return Vec::new();
            }
            return self.set_focus(tree.auto_focus_hit());
        }
        if let Some(hit) = tree.auto_focus_hit() {
            return self.set_focus(Some(hit));
        }
        Vec::new()
    }

    pub fn sync_interaction_for_tree(&mut self, tree: &HostTree) -> Vec<UiEvent> {
        let mut events = Vec::new();
        if self
            .state
            .hovered
            .as_ref()
            .is_some_and(|id| tree.node(id).map_or(true, |node| !node.event_policy.hover))
        {
            let previous = self.state.hovered.clone();
            self.state.hovered = None;
            events.push(UiEvent::HoverChanged {
                previous,
                current: None,
            });
        }
        if self
            .state
            .pressed
            .as_ref()
            .is_some_and(|id| tree.node(id).map_or(true, |node| !node.event_policy.press))
        {
            let previous = self.state.pressed.clone();
            self.state.pressed = None;
            self.pressed_hit = None;
            events.push(UiEvent::PressedChanged {
                previous,
                current: None,
            });
        }
        events
    }

    pub fn focus_node(&mut self, tree: &HostTree, id: &UiId) -> Vec<UiEvent> {
        self.set_focus(tree.focusable_hit(id))
    }

    fn set_focus(&mut self, current: Option<HitResult>) -> Vec<UiEvent> {
        let previous = self.state.focused.clone();
        self.state.focused = current.as_ref().map(|hit| hit.id.clone());
        if previous == self.state.focused {
            return Vec::new();
        }
        vec![UiEvent::FocusChanged { previous, current }]
    }

    fn pointer_leave(&mut self) -> Vec<UiEvent> {
        let previous = self.state.hovered.clone();
        self.state.hovered = None;
        self.state.pressed = None;
        self.pressed_hit = None;
        vec![UiEvent::PointerLeft { previous }]
    }
}

impl Default for UiEventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_events_to_animations(
    tree: &HostTree,
    animations: &mut AnimationRegistry,
    events: impl IntoIterator<Item = UiEvent>,
) -> bool {
    let mut changed = false;
    for event in events {
        match event {
            UiEvent::HoverChanged { previous, current } => {
                if let Some(id) = previous {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        id,
                        AnimProperty::Hover,
                        false,
                    );
                }
                if let Some(hit) = current {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        hit.id,
                        AnimProperty::Hover,
                        true,
                    );
                }
            }
            UiEvent::PressedChanged { previous, current } => {
                if let Some(id) = previous {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        id,
                        AnimProperty::Pressed,
                        false,
                    );
                }
                if let Some(hit) = current {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        hit.id,
                        AnimProperty::Pressed,
                        true,
                    );
                }
            }
            UiEvent::Clicked(hit) => {
                changed |=
                    set_node_animation_targets(tree, animations, hit.id, AnimProperty::Focus, true);
            }
            UiEvent::Wheel { .. } => {}
            UiEvent::TextInput { .. }
            | UiEvent::ImeStarted { .. }
            | UiEvent::ImeUpdated { .. }
            | UiEvent::ImeEnded { .. }
            | UiEvent::Backspace { .. }
            | UiEvent::KeyDown { .. }
            | UiEvent::PointerPressed { .. }
            | UiEvent::PointerMoved { .. }
            | UiEvent::PointerDragged { .. }
            | UiEvent::PointerReleased { .. } => {}
            UiEvent::FocusChanged { previous, current } => {
                if let Some(id) = previous {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        id,
                        AnimProperty::Focus,
                        false,
                    );
                }
                if let Some(hit) = current {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        hit.id,
                        AnimProperty::Focus,
                        true,
                    );
                }
            }
            UiEvent::PointerLeft { previous } => {
                if let Some(id) = previous {
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        id.clone(),
                        AnimProperty::Hover,
                        false,
                    );
                    changed |= set_node_animation_targets(
                        tree,
                        animations,
                        id,
                        AnimProperty::Pressed,
                        false,
                    );
                }
            }
        }
    }
    changed
}

fn set_node_animation_targets(
    tree: &HostTree,
    animations: &mut AnimationRegistry,
    id: UiId,
    property: AnimProperty,
    active: bool,
) -> bool {
    let Some(node) = tree.node(&id) else {
        return false;
    };
    if !event_policy_allows(node.event_policy, property) {
        return false;
    }
    let mut changed = false;
    for binding in node
        .animation_bindings
        .iter()
        .copied()
        .filter(|binding| binding.property == property)
    {
        changed |= animations.set_binding_target(id.clone(), binding, active);
    }
    changed
}

fn event_policy_allows(policy: super::EventPolicy, property: AnimProperty) -> bool {
    match property {
        AnimProperty::Hover => policy.hover,
        AnimProperty::Pressed => policy.press,
        AnimProperty::Focus => policy.focus,
        _ => true,
    }
}
