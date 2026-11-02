use crate::core::*;
use std::sync::Arc;
fn node(id: &str, rect: UiRect) -> UiNode {
    UiNode::new(UiId::owned(id), UiNodeKind::Panel, rect)
}
fn tree() -> HostTree {
    let mut t = HostTree::new();
    t.push(node("root", UiRect::new(0.0, 0.0, 200.0, 100.0)));
    t
}
fn rect(t: &HostTree, id: &str) -> UiRect {
    t.node(&UiId::owned(id)).unwrap().layout_rect
}
#[test]
fn absolute_and_layer_are_out_of_flow_and_have_predictable_z_order() {
    let mut t = tree();
    t.node_mut(&UiId::owned("root")).unwrap().layout = LayoutSpec::Flex(FlexLayout::default());
    t.push(node("normal", UiRect::new(0.0, 0.0, 20.0, 20.0)).parent(UiId::owned("root")));
    t.push(
        node("absolute", UiRect::new(0.0, 0.0, 30.0, 20.0))
            .parent(UiId::owned("root"))
            .position(Position::Absolute(Point::new(10.0, 15.0)))
            .z_index(5)
            .interaction(InteractionRole::Button),
    );
    t.push(
        node("layer", UiRect::new(0.0, 0.0, 40.0, 20.0))
            .parent(UiId::owned("root"))
            .position(Position::Layer {
                horizontal: Align::Center,
                vertical: Align::End,
                offset: Point::new(3.0, -2.0),
            }),
    );
    t.push(node("next", UiRect::new(0.0, 0.0, 20.0, 20.0)).parent(UiId::owned("root")));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "next").left, 20.0);
    assert_eq!(rect(&t, "absolute"), UiRect::new(10.0, 15.0, 40.0, 35.0));
    assert_eq!(rect(&t, "layer"), UiRect::new(83.0, 78.0, 123.0, 98.0));
    assert_eq!(
        t.hit_test(Point::new(15.0, 20.0)).unwrap().id,
        UiId::owned("absolute")
    );
}
#[test]
fn anchors_flip_then_shift_without_cross_axis_overflow_preventing_the_flip() {
    let mut t = tree();
    t.push(
        node("button", UiRect::new(150.0, 80.0, 180.0, 95.0))
            .parent(UiId::owned("root"))
            .public_id("button"),
    );
    t.push(
        node("popup", UiRect::new(0.0, 0.0, 80.0, 40.0))
            .parent(UiId::owned("root"))
            .position(Position::anchor("button", AnchorSide::Bottom)),
    );
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "popup"), UiRect::new(120.0, 40.0, 200.0, 80.0));
}
#[test]
fn anchor_dependencies_recompute_when_only_the_target_moves() {
    let mut t = tree();
    let target = |x| {
        node("target", UiRect::new(x, 10.0, x + 20.0, 30.0))
            .parent(UiId::owned("root"))
            .public_id("target")
    };
    t.push(target(10.0));
    t.push(
        node("popup", UiRect::new(0.0, 0.0, 50.0, 20.0))
            .parent(UiId::owned("root"))
            .position(Position::anchor("target", AnchorSide::Bottom)),
    );
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut t);
    t.upsert(target(80.0));
    runtime.update(&mut t);
    assert_eq!(rect(&t, "popup").left, 80.0);
}
#[test]
fn missing_and_cyclic_anchors_report_errors() {
    let mut t = tree();
    t.push(
        node("a", UiRect::new(0.0, 0.0, 20.0, 20.0))
            .parent(UiId::owned("root"))
            .public_id("a")
            .position(Position::anchor("b", AnchorSide::Bottom)),
    );
    let mut r = LayoutRuntime::new();
    r.update(&mut t);
    assert_eq!(
        r.position_errors(),
        &[PositionError::MissingAnchor(Arc::from("b"))]
    );
    t.push(
        node("b", UiRect::new(0.0, 0.0, 20.0, 20.0))
            .parent(UiId::owned("root"))
            .public_id("b")
            .position(Position::anchor("a", AnchorSide::Bottom)),
    );
    r.update(&mut t);
    assert!(r.position_errors().contains(&PositionError::AnchorCycle));
}
#[test]
fn sticky_headers_follow_scroll_edges_and_return_to_their_flow_position() {
    let mut t = HostTree::new();
    let scroll = |y: f32| {
        UiNode::new(
            UiId::owned("scroll"),
            UiNodeKind::Clip,
            UiRect::new(0.0, 0.0, 200.0, 100.0),
        )
        .clip(UiRect::new(0.0, 0.0, 200.0, 100.0), 0.0, -y)
    };
    t.push(scroll(40.0));
    t.push(
        node("header", UiRect::new(0.0, 20.0, 200.0, 40.0))
            .parent(UiId::owned("scroll"))
            .position(Position::Sticky {
                top: Dimension::Points(0.0),
                left: Dimension::Auto,
            }),
    );
    let mut r = LayoutRuntime::new();
    r.update(&mut t);
    assert_eq!(rect(&t, "header").top - 40.0, 0.0);
    t.upsert(scroll(0.0));
    r.update(&mut t);
    assert_eq!(rect(&t, "header").top, 20.0);
}
#[test]
fn hidden_subtrees_do_not_paint_or_accept_pointer_or_focus() {
    let mut t = tree();
    t.push(
        node("hidden", UiRect::new(0.0, 0.0, 100.0, 100.0))
            .parent(UiId::owned("root"))
            .visibility(Visibility::Hidden),
    );
    t.push(
        node("button", UiRect::new(0.0, 0.0, 20.0, 20.0))
            .parent(UiId::owned("hidden"))
            .interaction(InteractionRole::Button)
            .style(VisualStyle::filled(Color::WHITE)),
    );
    apply_layout_tree(&mut t);
    assert!(t.hit_test(Point::new(5.0, 5.0)).is_none());
    assert!(t.focusable_hits().is_empty());
    assert!(
        !compile_scene(&t)
            .commands()
            .iter()
            .any(|p| matches!(p,ScenePrimitive::Rect{id,..}if id==&UiId::owned("button")))
    );
    assert!(!t.is_visible(&UiId::owned("missing")));
}
#[test]
fn clipping_is_used_by_both_full_and_retained_scene_roots() {
    let mut t = tree();
    t.push(
        node("clip", UiRect::new(0.0, 0.0, 30.0, 30.0))
            .parent(UiId::owned("root"))
            .overflow(Overflow::Clip)
            .style(VisualStyle::filled(Color::WHITE)),
    );
    t.push(
        node("button", UiRect::new(20.0, 0.0, 60.0, 20.0))
            .parent(UiId::owned("clip"))
            .interaction(InteractionRole::Button),
    );
    apply_layout_tree(&mut t);
    assert!(t.hit_test(Point::new(50.0, 5.0)).is_none());
    let root = compile_scene_root(&t, &UiId::owned("clip"));
    assert!(
        matches!(root.commands().first(),Some(ScenePrimitive::Clip{rect,..})if rect.width()==30.0)
    );
}
#[test]
fn collapsed_overflow_does_not_reserve_a_flex_gap() {
    let mut t = tree();
    t.node_mut(&UiId::owned("root")).unwrap().layout = LayoutSpec::Flex(FlexLayout {
        gap: 5.0,
        ..Default::default()
    });
    t.push(
        node("small-only", UiRect::new(0.0, 0.0, 30.0, 20.0))
            .parent(UiId::owned("root"))
            .overflow(Overflow::Collapse(Size::new(250.0, 0.0))),
    );
    t.push(node("normal", UiRect::new(0.0, 0.0, 30.0, 20.0)).parent(UiId::owned("root")));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "normal").left, 0.0);
    assert!(!t.is_visible(&UiId::owned("small-only")));
}
#[test]
fn grid_consumes_the_same_child_constraints_as_other_layouts() {
    let mut t = tree();
    t.node_mut(&UiId::owned("root")).unwrap().layout =
        LayoutSpec::Grid(GridLayout::new(vec![GridTrack::Fraction(1.0)]));
    t.push(
        node("cell", UiRect::default())
            .parent(UiId::owned("root"))
            .size_constraints(
                SizeConstraints::new(Dimension::Percent(0.5), Dimension::Points(10.0))
                    .max_size(Size::new(60.0, 20.0)),
            ),
    );
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "cell").width(), 60.0);
    assert_eq!(rect(&t, "cell").height(), 10.0);
}
#[test]
fn public_visibility_events_include_removal() {
    let mut before = tree();
    before.push(
        node("chip", UiRect::default())
            .parent(UiId::owned("root"))
            .public_id("chip"),
    );
    let mut after = before.clone();
    after.upsert(
        node("chip", UiRect::default())
            .parent(UiId::owned("root"))
            .public_id("chip")
            .visibility(Visibility::Hidden),
    );
    assert_eq!(
        after.visibility_events(&before)[0].payload,
        UiEventPayload::Visibility { visible: false }
    );
    assert!(after.public_node("chip").unwrap().semantics.state.hidden);
}
