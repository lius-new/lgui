use lgui_core::core::{
    HostTree, InteractionRole, UiEvent, UiEventKind, UiEventPayload, UiId, UiNode, UiNodeKind,
    UiRect, UiRuntime,
};

#[test]
fn pending_focus_request_exposes_focus_event_handlers() {
    let target = UiId::new("queued-focus-target");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            target.clone(),
            UiNodeKind::Button,
            UiRect::new(0.0, 0.0, 100.0, 40.0),
        )
        .interaction(InteractionRole::Button)
        .on_event(UiEventKind::Focus, |_context, _payload| {}),
    );

    let mut runtime = UiRuntime::new();
    runtime.hook_updates().request_focus(target.clone());
    let output = runtime.apply_pending_updates(&tree);

    assert!(output.focus_changed);
    assert_eq!(runtime.interaction_state().focused, Some(target));
    assert!(matches!(
        output.focus_events.as_slice(),
        [UiEvent::FocusChanged {
            previous: None,
            current: Some(_),
        }]
    ));
    let handler_events = output
        .focus_events
        .iter()
        .flat_map(|event| tree.handler_events(event))
        .collect::<Vec<_>>();
    assert_eq!(handler_events.len(), 1);
    assert_eq!(handler_events[0].payload, UiEventPayload::Focus);
}
