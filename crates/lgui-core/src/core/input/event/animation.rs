use super::*;

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
            | UiEvent::Keyboard { .. }
            | UiEvent::PointerPressed { .. }
            | UiEvent::PointerMoved { .. }
            | UiEvent::PointerDragged { .. }
            | UiEvent::PointerReleased { .. }
            | UiEvent::SemanticValue { .. }
            | UiEvent::SemanticAction { .. } => {}
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
