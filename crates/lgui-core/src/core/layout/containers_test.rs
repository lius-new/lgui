use crate::core::*;
fn root(layout: LayoutSpec, w: f32, h: f32) -> HostTree {
    let mut t = HostTree::new();
    t.push(
        UiNode::new(
            UiId::owned("root"),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, w, h),
        )
        .layout(layout),
    );
    t
}
fn child(id: &str, w: f32, h: f32) -> UiNode {
    UiNode::new(
        UiId::owned(id),
        UiNodeKind::Panel,
        UiRect::new(0.0, 0.0, w, h),
    )
    .parent(UiId::owned("root"))
}
fn rect(t: &HostTree, id: &str) -> UiRect {
    t.node(&UiId::owned(id)).unwrap().layout_rect
}
#[test]
fn grow_redistributes_after_a_maximum_freezes_an_item() {
    let mut t = root(LayoutSpec::Flex(FlexLayout::default()), 300.0, 30.0);
    t.push(
        child("a", 50.0, 20.0)
            .flex_item(FlexItem::grow(1.0))
            .size_constraints(SizeConstraints::default().max_size(Size::new(80.0, 30.0))),
    );
    t.push(child("b", 50.0, 20.0).flex_item(FlexItem::grow(1.0)));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "a").width(), 80.0);
    assert_eq!(rect(&t, "b").width(), 220.0);
}
#[test]
fn shrink_is_weighted_by_basis_and_respects_minimums() {
    let mut t = root(LayoutSpec::Flex(FlexLayout::default()), 120.0, 30.0);
    t.push(
        child("a", 100.0, 20.0)
            .size_constraints(SizeConstraints::default().min_size(Size::new(80.0, 0.0))),
    );
    t.push(child("b", 100.0, 20.0));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "a").width(), 80.0);
    assert_eq!(rect(&t, "b").width(), 40.0);
}
#[test]
fn wrapping_retains_order_and_line_gaps() {
    let f = FlexLayout {
        wrap: true,
        gap: 5.0,
        ..Default::default()
    };
    let mut t = root(LayoutSpec::Flex(f), 100.0, 100.0);
    for id in ["a", "b", "c"] {
        t.push(child(id, 45.0, 20.0));
    }
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "b").left, 50.0);
    assert_eq!(rect(&t, "c").top, 25.0);
}
#[test]
fn vertical_justification_and_self_alignment() {
    let mut f = FlexLayout::new(Axis::Vertical);
    f.justify = Justify::SpaceBetween;
    f.align = Align::End;
    let mut t = root(LayoutSpec::Flex(f), 100.0, 100.0);
    t.push(child("a", 20.0, 20.0));
    t.push(child("b", 20.0, 20.0).flex_item(FlexItem {
        align_self: Some(Align::Center),
        ..Default::default()
    }));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "a"), UiRect::new(80.0, 0.0, 100.0, 20.0));
    assert_eq!(rect(&t, "b"), UiRect::new(40.0, 80.0, 60.0, 100.0));
}
#[test]
fn dock_reserves_edges_then_fills_remaining() {
    let mut t = root(
        LayoutSpec::Dock {
            padding: EdgeInsets::all(5.0),
            gap: Dimension::Points(2.0),
        },
        200.0,
        100.0,
    );
    t.push(child("top", 0.0, 20.0).dock(DockSide::Top));
    t.push(child("left", 30.0, 0.0).dock(DockSide::Left));
    t.push(child("fill", 0.0, 0.0));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "top"), UiRect::new(5.0, 5.0, 195.0, 25.0));
    assert_eq!(rect(&t, "fill"), UiRect::new(37.0, 27.0, 195.0, 95.0));
}
#[test]
fn grid_fixed_auto_fraction_tracks_and_spans() {
    let mut g = GridLayout::new(vec![
        GridTrack::Points(40.0),
        GridTrack::Content,
        GridTrack::Fraction(1.0),
        GridTrack::Fraction(2.0),
    ]);
    g.gap = Size::new(10.0, 5.0);
    let mut t = root(LayoutSpec::Grid(g), 400.0, 100.0);
    for (i, id) in ["a", "b", "c", "d"].iter().enumerate() {
        t.push(child(id, if i == 1 { 30.0 } else { 0.0 }, 20.0));
    }
    t.push(child("span", 50.0, 30.0).grid_cell(GridCell::at(2, 1).span(2, 1)));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "b").width(), 30.0);
    assert_eq!(rect(&t, "c").width(), 100.0);
    assert_eq!(rect(&t, "d").width(), 200.0);
    assert_eq!(rect(&t, "span").width(), 310.0);
    assert_eq!(rect(&t, "span").top, 25.0);
}
#[test]
fn grid_auto_placement_skips_spanning_cells() {
    let g = GridLayout::new(vec![GridTrack::Fraction(1.0); 3]);
    let mut t = root(LayoutSpec::Grid(g), 300.0, 100.0);
    t.push(child("span", 0.0, 20.0).grid_cell(GridCell {
        column_span: 2,
        ..Default::default()
    }));
    t.push(child("next", 0.0, 20.0));
    t.push(child("last", 0.0, 20.0));
    apply_layout_tree(&mut t);
    assert_eq!(rect(&t, "next").left, 200.0);
    assert_eq!(rect(&t, "next").top, 0.0);
    assert_eq!(rect(&t, "last").top, 20.0);
}

#[test]
fn oversized_right_dock_keeps_its_trailing_edge_and_leaves_no_negative_fill() {
    let mut tree = root(
        LayoutSpec::Dock {
            padding: EdgeInsets::ZERO,
            gap: Dimension::Points(0.0),
        },
        50.0,
        30.0,
    );
    tree.push(child("right", 80.0, 10.0).dock(DockSide::Right));
    tree.push(child("fill", 0.0, 0.0));
    apply_layout_tree(&mut tree);
    assert_eq!(rect(&tree, "right"), UiRect::new(-30.0, 0.0, 50.0, 30.0));
    assert_eq!(rect(&tree, "fill").width(), 0.0);
}
