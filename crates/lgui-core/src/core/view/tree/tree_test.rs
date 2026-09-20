use super::*;
use crate::core::{AnimProperty, AnimationBinding, CursorIcon, RenderPhase, UiNodeKind, VisualStyle};

#[test]
fn retained_clone_shares_unchanged_nodes_and_detaches_only_updated_nodes() {
    let first_id = UiId::new("first");
    let second_id = UiId::new("second");
    let mut original = HostTree::new();
    original.push(UiNode::new(
        first_id.clone(),
        UiNodeKind::Panel,
        UiRect::new(0.0, 0.0, 10.0, 10.0),
    ));
    original.push(UiNode::new(
        second_id.clone(),
        UiNodeKind::Panel,
        UiRect::new(10.0, 0.0, 20.0, 10.0),
    ));
    original.take_projection_changes();

    let mut next = original.clone();
    assert!(Arc::ptr_eq(&original.nodes[0], &next.nodes[0]));
    assert!(Arc::ptr_eq(&original.nodes[1], &next.nodes[1]));

    next.upsert(
        UiNode::new(
            first_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 10.0, 10.0),
        )
        .style(VisualStyle::filled(crate::core::Color::WHITE)),
    );

    assert!(!Arc::ptr_eq(&original.nodes[0], &next.nodes[0]));
    assert!(Arc::ptr_eq(&original.nodes[1], &next.nodes[1]));
    assert_ne!(
        original.node(&first_id).unwrap().style,
        next.node(&first_id).unwrap().style
    );
    assert!(next.node(&second_id).is_some());
}

#[test]
fn runtime_sync_changes_track_only_relevant_nodes() {
    let plain_id = UiId::new("plain");
    let animated_id = UiId::new("animated");
    let mut tree = HostTree::new();
    tree.push(UiNode::new(
        plain_id.clone(),
        UiNodeKind::Panel,
        UiRect::new(0.0, 0.0, 10.0, 10.0),
    ));
    tree.push(
        UiNode::new(
            animated_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(10.0, 0.0, 20.0, 10.0),
        )
        .animation(AnimationBinding::new(AnimProperty::Opacity, 0.0, 1.0))
        .animation_target(AnimProperty::Opacity, false),
    );
    assert_eq!(tree.animation_sync_ids().count(), 1);
    tree.take_projection_changes();

    tree.upsert(
        UiNode::new(
            plain_id,
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 10.0, 10.0),
        )
        .style(VisualStyle::filled(crate::core::Color::WHITE)),
    );

    assert_eq!(tree.animation_sync_ids().count(), 0);
    assert!(!tree.needs_focus_sync());
}

#[test]
fn popup_hit_testing_wins_over_later_siblings_and_ancestor_clips() {
    let clip_id = UiId::new("clip");
    let popup_id = UiId::new("popup");
    let sibling_id = UiId::new("sibling");
    let target = UiRect::new(20.0, 20.0, 40.0, 40.0);
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            clip_id.clone(),
            UiNodeKind::Clip,
            UiRect::new(0.0, 0.0, 10.0, 10.0),
        )
        .clip(UiRect::new(0.0, 0.0, 10.0, 10.0), 0.0, 0.0),
    );
    tree.push(
        UiNode::new(popup_id.clone(), UiNodeKind::Button, target)
            .parent(clip_id)
            .interaction(InteractionRole::Button)
            .render_phase(RenderPhase::Popup),
    );
    tree.push(
        UiNode::new(sibling_id, UiNodeKind::Button, target)
            .interaction(InteractionRole::Button)
            .render_phase(RenderPhase::Overlay),
    );

    let hit = tree
        .hit_test(Point::new(30.0, 30.0))
        .expect("popup should be hittable outside its ancestor clip");
    assert_eq!(hit.id, popup_id);
}

#[test]
fn interactive_titlebar_children_take_precedence_over_the_drag_region() {
    let drag_id = UiId::new("titlebar");
    let button_id = UiId::new("close");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            drag_id.clone(),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, 400.0, 64.0),
        )
        .interaction(InteractionRole::WindowDragRegion)
        .event_policy(crate::core::EventPolicy::NONE),
    );
    tree.push(
        UiNode::new(
            button_id.clone(),
            UiNodeKind::Button,
            UiRect::new(350.0, 0.0, 400.0, 64.0),
        )
        .parent(drag_id.clone())
        .interaction(InteractionRole::Button),
    );

    let drag = tree
        .hit_test(Point::new(100.0, 32.0))
        .expect("titlebar background should be draggable");
    assert_eq!(drag.id, drag_id);
    assert_eq!(drag.interaction, InteractionRole::WindowDragRegion);

    let button = tree
        .hit_test(Point::new(375.0, 32.0))
        .expect("titlebar button should remain interactive");
    assert_eq!(button.id, button_id);
    assert_eq!(button.interaction, InteractionRole::Button);
}

#[test]
fn cursor_resolution_inherits_from_ancestors_and_respects_overrides() {
    let root_id = UiId::new("root");
    let field_id = UiId::new("field");
    let link_id = UiId::new("link");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            root_id.clone(),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .cursor(CursorIcon::Text),
    );
    tree.push(
        UiNode::new(
            field_id.clone(),
            UiNodeKind::Group,
            UiRect::new(10.0, 10.0, 90.0, 90.0),
        )
        .parent(root_id.clone()),
    );
    tree.push(
        UiNode::new(
            link_id.clone(),
            UiNodeKind::Button,
            UiRect::new(10.0, 10.0, 50.0, 50.0),
        )
        .parent(field_id.clone())
        .cursor(CursorIcon::Pointer),
    );

    // Inside `field` but outside `link`: inherits the root's I-beam cursor.
    assert_eq!(
        tree.cursor_at(Point::new(60.0, 60.0)),
        Some(CursorIcon::Text)
    );
    // Inside `link`: the descendant override wins.
    assert_eq!(
        tree.cursor_at(Point::new(30.0, 30.0)),
        Some(CursorIcon::Pointer)
    );
    // Outside every node.
    assert_eq!(tree.cursor_at(Point::new(150.0, 150.0)), None);
}
