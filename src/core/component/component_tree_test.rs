use super::*;

#[test]
fn keyed_children_keep_identity_when_order_changes() {
    let tree = ComponentTree::new();
    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    let first = tree.keyed_child(root, 1, 10, "row");
    let second = tree.keyed_child(root, 1, 20, "row");
    tree.finish_component(first);
    tree.finish_component(second);
    tree.finish_component(root);
    tree.end_render();

    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    let second_again = tree.keyed_child(root, 1, 20, "row");
    let first_again = tree.keyed_child(root, 1, 10, "row");

    assert_eq!(first, first_again);
    assert_eq!(second, second_again);
}

#[test]
fn changing_component_type_replaces_the_instance_without_borrow_reentrancy() {
    let tree = ComponentTree::new();
    tree.begin_render();
    let first = tree.root(UiId::owned("root"), "first");
    tree.finish_component(first);
    tree.end_render();

    tree.begin_render();
    let second = tree.root(UiId::owned("root"), "second");
    tree.finish_component(second);
    tree.end_render();

    assert_ne!(first, second);
    assert!(!tree.is_alive(first));
    assert!(tree.is_alive(second));
}

#[test]
fn abort_render_restores_replaced_component_identity() {
    let tree = ComponentTree::new();
    tree.begin_render();
    let first = tree.root(UiId::owned("root"), "first");
    tree.finish_component(first);
    tree.end_render();

    tree.begin_render();
    let replacement = tree.root(UiId::owned("root"), "replacement");
    assert_ne!(first, replacement);
    tree.abort_render();

    tree.begin_render();
    let restored = tree.root(UiId::owned("root"), "first");
    assert_eq!(restored, first);
    assert!(!tree.is_alive(replacement));
}

#[test]
fn abort_render_preserves_committed_props_output_and_dirty_flags() {
    let tree = ComponentTree::new();
    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    tree.begin_component_execution(root);
    tree.commit_output(
        root,
        1_u32,
        UiElement::group(
            UiId::owned("committed"),
            super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
        ),
    );
    tree.finish_component(root);
    tree.end_render();

    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    tree.begin_component_execution(root);
    tree.commit_output(
        root,
        2_u32,
        UiElement::group(
            UiId::owned("abandoned"),
            super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
        ),
    );
    tree.finish_component(root);
    tree.abort_render();

    assert!(tree.can_reuse(root, &1_u32));
    assert!(!tree.can_reuse(root, &2_u32));
    assert_eq!(
        tree.reuse_output(root).into_parts().0.id.as_str(),
        "committed"
    );

    assert!(tree.mark_dirty(root));
    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    tree.finish_component(root);
    tree.abort_render();
    assert!(tree.is_dirty(root));
}

#[test]
#[should_panic(expected = "hook order changed")]
fn hook_order_changes_are_rejected() {
    let tree = ComponentTree::new();
    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    tree.record_hook(root, HookSlotKind::State);
    tree.finish_component(root);
    tree.end_render();

    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    tree.record_hook(root, HookSlotKind::Effect);
    tree.finish_component(root);
}

#[test]
fn dirty_branch_executes_while_an_unchanged_sibling_reuses_its_output() {
    let tree = ComponentTree::new();
    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    tree.begin_component_execution(root);
    let first = tree.positioned_child(root, 1, 0, "child");
    tree.begin_component_execution(first);
    tree.commit_output(
        first,
        1_u32,
        UiElement::group(
            UiId::owned("first"),
            super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
        ),
    );
    tree.finish_component(first);
    let second = tree.positioned_child(root, 1, 1, "child");
    tree.begin_component_execution(second);
    tree.commit_output(
        second,
        2_u32,
        UiElement::group(
            UiId::owned("second"),
            super::super::UiRect::new(1.0, 0.0, 1.0, 1.0),
        ),
    );
    tree.finish_component(second);
    tree.finish_component(root);
    tree.end_render();

    assert!(tree.mark_dirty(first));
    tree.begin_render();
    let root = tree.root(UiId::owned("root"), "root");
    assert!(!tree.begin_component_execution(root));
    let first_again = tree.positioned_child(root, 1, 0, "child");
    assert!(!tree.can_reuse(first_again, &1_u32));
    tree.begin_component_execution(first_again);
    tree.commit_output(
        first_again,
        1_u32,
        UiElement::group(
            UiId::owned("first"),
            super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
        ),
    );
    tree.finish_component(first_again);
    let second_again = tree.positioned_child(root, 1, 1, "child");
    assert!(tree.can_reuse(second_again, &2_u32));
    tree.reuse_output(second_again);
    tree.finish_component(root);
    tree.end_render();

    assert_eq!(tree.metrics().executed, 2);
}
