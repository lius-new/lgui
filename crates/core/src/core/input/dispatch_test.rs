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
        &ApplicationContext::empty(crate::memory::test_memory_options()),
        &WindowId::new("test"),
        |action| runtime.handle_default_action(&tree, action),
        |_| {},
    );

    assert_eq!(&*calls.lock().unwrap(), &["capture", "target", "parent"]);
}
