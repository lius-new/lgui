use super::*;
use crate::core::{EdgeInsets, HostTree, UiNode, UiNodeKind};

#[test]
fn layout_tree_positions_a_stack_and_translates_descendants() {
    let root_id = UiId::owned("root");
    let child_id = UiId::owned("child");
    let grandchild_id = UiId::owned("grandchild");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            root_id.clone(),
            UiNodeKind::Group,
            UiRect::new(10.0, 20.0, 210.0, 220.0),
        )
        .layout(LayoutSpec::Stack {
            axis: Axis::Vertical,
            gap: 4.0,
            padding: EdgeInsets::all(8.0),
            align: Align::Start,
        }),
    );
    tree.push(
        UiNode::new(
            child_id.clone(),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, 40.0, 30.0),
        )
        .parent(root_id)
        .layout(LayoutSpec::Fixed(Size::new(40.0, 30.0))),
    );
    tree.push(
        UiNode::new(
            grandchild_id.clone(),
            UiNodeKind::Text,
            UiRect::new(5.0, 5.0, 15.0, 15.0),
        )
        .parent(child_id.clone()),
    );

    apply_layout_tree(&mut tree);

    assert_eq!(
        tree.node(&child_id).unwrap().layout_rect,
        UiRect::new(18.0, 28.0, 58.0, 58.0)
    );
    assert_eq!(
        tree.node(&grandchild_id).unwrap().layout_rect,
        UiRect::new(23.0, 33.0, 33.0, 43.0)
    );
}

#[test]
fn retained_layout_reuses_results_when_only_paint_content_changes() {
    let id = UiId::owned("text");
    let mut runtime = LayoutRuntime::new();
    let mut first = HostTree::new();
    first.push(UiNode::new(
        id.clone(),
        UiNodeKind::Text,
        UiRect::new(0.0, 0.0, 20.0, 10.0),
    ));
    assert_eq!(runtime.update(&mut first).laid_out_nodes, 1);

    let mut second = HostTree::new();
    second.push(
        UiNode::new(id, UiNodeKind::Text, UiRect::new(0.0, 0.0, 20.0, 10.0)).text(
            "changed",
            super::super::TextStyle::new(super::super::Color::WHITE, 12.0, 400),
        ),
    );
    let metrics = runtime.update(&mut second);
    assert_eq!(metrics.laid_out_nodes, 0);
    assert_eq!(metrics.reused_nodes, 1);
}

#[test]
fn leaf_geometry_change_lays_out_only_its_nearest_stack_boundary() {
    let left = UiId::owned("left-stack");
    let left_child = UiId::owned("left-child");
    let right = UiId::owned("right-stack");
    let right_child = UiId::owned("right-child");
    let build = |left_width: f32| {
        let mut tree = HostTree::new();
        for (id, rect) in [
            (left.clone(), UiRect::new(0.0, 0.0, 100.0, 100.0)),
            (right.clone(), UiRect::new(100.0, 0.0, 200.0, 100.0)),
        ] {
            tree.push(
                UiNode::new(id, UiNodeKind::Group, rect).layout(LayoutSpec::Stack {
                    axis: Axis::Vertical,
                    gap: 0.0,
                    padding: EdgeInsets::all(0.0),
                    align: Align::Start,
                }),
            );
        }
        tree.push(
            UiNode::new(
                left_child.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, left_width, 20.0),
            )
            .parent(left.clone())
            .layout(LayoutSpec::Fixed(Size::new(left_width, 20.0))),
        );
        tree.push(
            UiNode::new(
                right_child.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 40.0, 20.0),
            )
            .parent(right.clone())
            .layout(LayoutSpec::Fixed(Size::new(40.0, 20.0))),
        );
        tree
    };
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut build(30.0));

    let metrics = runtime.update(&mut build(50.0));

    assert_eq!(metrics.laid_out_nodes, 2);
    assert_eq!(metrics.reused_nodes, 2);
}

#[test]
fn inserting_a_stack_child_invalidates_its_layout_boundary() {
    let root = UiId::owned("stack");
    let first_child = UiId::owned("first");
    let mut first = HostTree::new();
    first.push(
        UiNode::new(
            root.clone(),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .layout(LayoutSpec::Stack {
            axis: Axis::Vertical,
            gap: 4.0,
            padding: EdgeInsets::all(0.0),
            align: Align::Start,
        }),
    );
    first.push(
        UiNode::new(
            first_child.clone(),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 20.0, 20.0),
        )
        .parent(root.clone())
        .layout(LayoutSpec::Fixed(Size::new(20.0, 20.0))),
    );
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut first);
    let mut second = first.clone();
    second.push(
        UiNode::new(
            UiId::owned("second"),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 20.0, 20.0),
        )
        .parent(root)
        .layout(LayoutSpec::Fixed(Size::new(20.0, 20.0))),
    );

    let metrics = runtime.update(&mut second);

    assert_eq!(metrics.laid_out_nodes, 3);
    assert_eq!(
        second.node(&UiId::owned("second")).unwrap().layout_rect.top,
        24.0
    );
}
