use std::collections::HashSet;

use crate::application::{ApplicationContext, WindowId};

use super::{
    HitResult, RuntimeOutput, UiDefaultAction, UiEvent, UiEventContext, UiHandlerEvent, UiRect,
};

/// Executes every handler and control default action produced by one runtime input pass.
///
/// Platform backends translate native input and provide the default-action callback. Application
/// crates may observe each completed context to consume typed host commands, while propagation and
/// control behavior remain owned by the GUI runtime.
pub fn dispatch_runtime_output(
    output: RuntimeOutput,
    application: &ApplicationContext,
    window: &WindowId,
    mut handle_default_action: impl FnMut(UiDefaultAction) -> RuntimeOutput,
    mut finish_context: impl FnMut(UiEventContext),
) -> RuntimeOutput {
    let mut pending = vec![output];
    let mut combined = empty_output();
    let mut default_prevented = HashSet::new();

    while let Some(output) = pending.pop() {
        for event in &output.action_events {
            let mut context = UiEventContext::new(application.clone(), window.clone());
            event.dispatch(&mut context);
            finish_context(context);
        }

        for event in &output.handler_events {
            let context = dispatch_event_handlers(event, application, window);
            if context.default_prevented() {
                default_prevented.insert(event.target.clone());
            }
            finish_context(context);
        }

        for event in &output.events {
            let hit = match event {
                UiEvent::Clicked(hit) | UiEvent::Wheel { hit, .. } => hit,
                _ => continue,
            };
            let Some(context) = dispatch_hit_handlers(hit, application, window) else {
                continue;
            };
            if context.default_prevented() {
                default_prevented.insert(hit.id.clone());
            }
            finish_context(context);
        }

        for action in output.default_actions.iter().cloned() {
            if !default_prevented.contains(&action.event_target) {
                pending.push(handle_default_action(action));
            }
        }

        merge_output(&mut combined, output);
    }

    combined
}

pub fn dispatch_hit_handlers(
    hit: &HitResult,
    application: &ApplicationContext,
    window: &WindowId,
) -> Option<UiEventContext> {
    if hit.capture_handlers.is_empty() && hit.bubble_handlers.is_empty() {
        return None;
    }
    let mut context = UiEventContext::new(application.clone(), window.clone());
    for handler in &hit.capture_handlers {
        handler(&mut context);
        if context.propagation_stopped() {
            return Some(context);
        }
    }
    for handler in &hit.bubble_handlers {
        handler(&mut context);
        if context.propagation_stopped() {
            break;
        }
    }
    Some(context)
}

pub fn dispatch_event_handlers(
    event: &UiHandlerEvent,
    application: &ApplicationContext,
    window: &WindowId,
) -> UiEventContext {
    let mut context = UiEventContext::new(application.clone(), window.clone());
    for handler in &event.capture_handlers {
        handler(&mut context, &event.payload);
        if context.propagation_stopped() {
            return context;
        }
    }
    for handler in &event.bubble_handlers {
        handler(&mut context, &event.payload);
        if context.propagation_stopped() {
            break;
        }
    }
    context
}

fn empty_output() -> RuntimeOutput {
    RuntimeOutput {
        events: Vec::new(),
        handler_events: Vec::new(),
        action_events: Vec::new(),
        default_actions: Vec::new(),
        dirty_bounds: None,
        animation_changed: false,
        route_changed: false,
    }
}

fn merge_output(combined: &mut RuntimeOutput, mut output: RuntimeOutput) {
    combined.events.append(&mut output.events);
    combined.handler_events.append(&mut output.handler_events);
    combined.action_events.append(&mut output.action_events);
    combined.default_actions.append(&mut output.default_actions);
    combined.dirty_bounds = union(combined.dirty_bounds, output.dirty_bounds);
    combined.animation_changed |= output.animation_changed;
    combined.route_changed |= output.route_changed;
}

fn union(left: Option<UiRect>, right: Option<UiRect>) -> Option<UiRect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::core::{
        HostTree, InputEvent, InteractionRole, Point, PointerButton, PointerData, UiId, UiNode,
        UiNodeKind,
    };

    #[test]
    fn click_handlers_are_executed_by_the_runtime_dispatch_loop() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut tree = HostTree::new();
        let parent = UiId::owned("parent");
        tree.push(
            UiNode::new(
                parent.clone(),
                UiNodeKind::Group,
                UiRect::new(0.0, 0.0, 100.0, 100.0),
            )
            .on_click_capture({
                let calls = Arc::clone(&calls);
                move |_| calls.lock().unwrap().push("capture")
            })
            .on_click({
                let calls = Arc::clone(&calls);
                move |_| calls.lock().unwrap().push("parent")
            }),
        );
        tree.push(
            UiNode::new(
                UiId::owned("button"),
                UiNodeKind::Button,
                UiRect::new(0.0, 0.0, 100.0, 100.0),
            )
            .parent(parent)
            .interaction(InteractionRole::Button)
            .on_click({
                let calls = Arc::clone(&calls);
                move |_| calls.lock().unwrap().push("target")
            }),
        );
        let mut runtime = super::super::UiRuntime::new();
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

        dispatch_runtime_output(
            output,
            &ApplicationContext::empty(),
            &WindowId::new("test"),
            |action| runtime.handle_default_action(&tree, action),
            |_| {},
        );

        assert_eq!(&*calls.lock().unwrap(), &["capture", "target", "parent"]);
    }
}
