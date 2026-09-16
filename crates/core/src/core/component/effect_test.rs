use std::{cell::Cell, rc::Rc};

use super::*;
use crate::core::{ComponentTree, HookSlotKind, UiId};

fn effect_id(tree: &ComponentTree) -> HookId {
    let component = tree.root(UiId::owned("effect-owner"), "effect-owner");
    HookId::new(component, 0, HookSlotKind::Effect)
}

#[test]
fn dependency_change_cleans_up_before_next_effect() {
    let registry = EffectRegistry::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let tree = ComponentTree::new();
    tree.begin_render();
    let id = effect_id(&tree);

    registry.begin_frame();
    let first_events = Rc::clone(&events);
    registry.register(id, 1_u32, move || {
        first_events.borrow_mut().push("run-1");
        let cleanup_events = Rc::clone(&first_events);
        move || cleanup_events.borrow_mut().push("cleanup-1")
    });
    tree.finish_component(id.component());
    tree.end_render();
    registry.end_frame(&tree);
    registry.run_pending();

    tree.begin_render();
    let id = effect_id(&tree);
    registry.begin_frame();
    let second_events = Rc::clone(&events);
    registry.register(id, 2_u32, move || {
        second_events.borrow_mut().push("run-2");
    });
    tree.finish_component(id.component());
    tree.end_render();
    registry.end_frame(&tree);
    registry.run_pending();

    assert_eq!(&*events.borrow(), &["run-1", "cleanup-1", "run-2"]);
}

#[test]
fn unmount_drops_an_effect_that_never_committed() {
    let registry = EffectRegistry::new();
    let runs = Rc::new(Cell::new(0));
    let tree = ComponentTree::new();
    tree.begin_render();
    let id = effect_id(&tree);

    registry.begin_frame();
    let effect_runs = Rc::clone(&runs);
    registry.register(id, (), move || {
        effect_runs.set(effect_runs.get() + 1);
    });
    tree.finish_component(id.component());
    tree.end_render();
    registry.end_frame(&tree);

    tree.begin_render();
    tree.end_render();
    registry.begin_frame();
    registry.end_frame(&tree);
    registry.run_pending();

    assert_eq!(runs.get(), 0);
}

#[test]
fn committed_effect_cleans_up_when_its_component_unmounts() {
    let registry = EffectRegistry::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let tree = ComponentTree::new();

    tree.begin_render();
    registry.begin_frame();
    let id = effect_id(&tree);
    let effect_events = Rc::clone(&events);
    registry.register(id, (), move || {
        effect_events.borrow_mut().push("mount");
        let cleanup_events = Rc::clone(&effect_events);
        move || cleanup_events.borrow_mut().push("unmount")
    });
    tree.finish_component(id.component());
    tree.end_render();
    registry.end_frame(&tree);
    registry.run_pending();

    tree.begin_render();
    tree.end_render();
    registry.begin_frame();
    registry.end_frame(&tree);
    registry.run_pending();

    assert_eq!(&*events.borrow(), &["mount", "unmount"]);
}

#[test]
fn abandoned_render_does_not_replace_a_committed_effect() {
    let registry = EffectRegistry::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let tree = ComponentTree::new();

    tree.begin_render();
    registry.begin_frame();
    let id = effect_id(&tree);
    let first_events = Rc::clone(&events);
    registry.register(id, 1_u32, move || {
        first_events.borrow_mut().push("run-1");
        let cleanup_events = Rc::clone(&first_events);
        move || cleanup_events.borrow_mut().push("cleanup-1")
    });
    tree.finish_component(id.component());
    tree.end_render();
    registry.end_frame(&tree);
    registry.run_pending();

    tree.begin_render();
    registry.begin_frame();
    let id = effect_id(&tree);
    let abandoned_events = Rc::clone(&events);
    registry.register(id, 2_u32, move || {
        abandoned_events.borrow_mut().push("run-abandoned");
    });
    tree.abandon_component(id.component());

    registry.begin_frame();
    registry.run_pending();
    assert_eq!(&*events.borrow(), &["run-1"]);

    registry.clear();
    assert_eq!(&*events.borrow(), &["run-1", "cleanup-1"]);
}
