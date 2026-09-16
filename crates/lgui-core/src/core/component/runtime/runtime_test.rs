use std::any::Any;

use super::*;
use crate::core::{
    compile_scene, AnimationBinding, ComponentActionOutcome, ComponentState,
    CompositingLayerAnimation, CompositingLayerSpec, ImeEvent, InputEvent, InteractionRole,
    KeyModifiers, KeyboardEvent, Point, PointerButton, PointerData, ScenePrimitive, UiNode,
    UiNodeKind, VisualStyle, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION, POINTER_UP_ACTION,
};

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

fn compositing_content_signature(scene: &crate::core::Scene) -> u64 {
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
        let unrelated_owner = components.root(UiId::owned("unrelated-owner"), "unrelated-owner");
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

    assert_eq!(runtime.frame_interval_ms(), Some(33));

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
        .style(VisualStyle::filled(crate::core::Color::WHITE)),
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

    assert_eq!(runtime.frame_interval_ms(), Some(16));

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
        .on_event(crate::core::UiEventKind::KeyDown, |context, _| {
            context.prevent_default();
        }),
    );
    let mut runtime = UiRuntime::new();
    assert!(runtime.focus_node(&tree, &id));

    let output = runtime.handle_input(&tree, key_down(NamedKey::Tab, KeyModifiers::default()));

    assert_eq!(output.handler_events.len(), 1);
    assert_eq!(output.default_actions.len(), 1);
    assert_eq!(runtime.interaction_state().focused, Some(id));
    let mut context = crate::core::UiEventContext::new(
        crate::application::ApplicationContext::empty(crate::memory::test_memory_options()),
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
        .on_event(crate::core::UiEventKind::Input, |context, _| {
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
        .on_event(crate::core::UiEventKind::Change, |_, _| {}),
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
        .on_event(crate::core::UiEventKind::CompositionStart, |_, _| {})
        .on_event(crate::core::UiEventKind::CompositionUpdate, |_, _| {})
        .on_event(crate::core::UiEventKind::CompositionEnd, |_, _| {})
        .on_event(crate::core::UiEventKind::Input, |_, _| {}),
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
    let commit = runtime.handle_input(&tree, InputEvent::Ime(ImeEvent::Commit("中文".to_owned())));
    let end = runtime.handle_input(&tree, InputEvent::Ime(ImeEvent::Disabled));

    assert_eq!(start.handler_events[0].target, input_id);
    assert!(matches!(
        update.handler_events[0].payload,
        crate::core::UiEventPayload::CompositionUpdate { ref text, ref cursor }
            if text == "nǐ" && cursor == &Some(1..1)
    ));
    assert_eq!(commit.default_actions.len(), 1);
    assert!(matches!(
        commit.handler_events[0].payload,
        crate::core::UiEventPayload::Input { ref text } if text == "中文"
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

    fn wants_frame(&self) -> bool {
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
