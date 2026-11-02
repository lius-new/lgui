use super::*;
use crate::core::{Color, Dimension, TextStyle, UiNode, UiNodeKind};
use crate::text::{
    TextLayout, TextLayoutRequest, TextMeasureRequest, TextMetrics, TextSystem, TextSystemHandle,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn stack(id: &str, rect: UiRect, axis: Axis, gap: f32, padding: f32, align: Align) -> UiNode {
    UiNode::new(UiId::owned(id), UiNodeKind::Group, rect).layout(LayoutSpec::Flex(
        crate::core::FlexLayout {
            axis,
            gap,
            padding: EdgeInsets::all(padding),
            align,
            justify: crate::core::Justify::Start,
            wrap: false,
        },
    ))
}

fn panel(id: &str, parent: &str, width: f32, height: f32) -> UiNode {
    UiNode::new(
        UiId::owned(id),
        UiNodeKind::Panel,
        UiRect::new(0.0, 0.0, width, height),
    )
    .parent(UiId::owned(parent))
}

fn rect(tree: &HostTree, id: &str) -> UiRect {
    tree.node(&UiId::owned(id)).unwrap().layout_rect
}

#[test]
fn percentages_are_measured_inside_parent_padding() {
    let mut tree = HostTree::new();
    tree.push(stack(
        "root",
        UiRect::new(10.0, 20.0, 310.0, 220.0),
        Axis::Horizontal,
        0.0,
        10.0,
        Align::Start,
    ));
    tree.push(
        panel("child", "root", 0.0, 0.0).size_constraints(SizeConstraints::new(
            Dimension::Percent(0.5),
            Dimension::Points(30.0),
        )),
    );
    apply_layout_tree(&mut tree);
    assert_eq!(rect(&tree, "child"), UiRect::new(20.0, 30.0, 160.0, 60.0));
    let child = tree.node(&UiId::owned("child")).unwrap();
    assert_eq!(child.hit_rect, child.layout_rect);
    assert_eq!(child.paint_bounds, child.layout_rect);
}

#[test]
fn stretch_respects_maximums_and_does_not_get_overwritten_by_recursion() {
    let mut tree = HostTree::new();
    tree.push(stack(
        "root",
        UiRect::new(0.0, 0.0, 300.0, 100.0),
        Axis::Horizontal,
        0.0,
        0.0,
        Align::Stretch,
    ));
    tree.push(
        panel("child", "root", 10.0, 10.0).size_constraints(
            SizeConstraints::new(Dimension::Points(40.0), Dimension::Auto)
                .max_size(Size::new(100.0, 30.0)),
        ),
    );
    apply_layout_tree(&mut tree);
    assert_eq!(rect(&tree, "child"), UiRect::new(0.0, 0.0, 40.0, 30.0));
}

#[test]
fn retained_percentage_child_reacts_to_parent_resize_without_rerendering() {
    let mut tree = HostTree::new();
    tree.push(UiNode::new(
        UiId::owned("root"),
        UiNodeKind::Group,
        UiRect::new(0.0, 0.0, 200.0, 100.0),
    ));
    tree.push(
        panel("child", "root", 80.0, 20.0).size_constraints(SizeConstraints::new(
            Dimension::Percent(0.5),
            Dimension::Auto,
        )),
    );
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "child").width(), 100.0);

    let mut parent = tree.node(&UiId::owned("root")).unwrap().clone();
    parent.layout_rect = UiRect::new(0.0, 0.0, 400.0, 100.0);
    parent.hit_rect = parent.layout_rect;
    parent.paint_bounds = parent.layout_rect;
    tree.upsert(parent);
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "child").width(), 200.0);

    // Removing percentage sizing restores the declared width, not the last output.
    tree.upsert(panel("child", "root", 80.0, 20.0).size_constraints(SizeConstraints::default()));
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "child").width(), 80.0);
}

struct MeasuredText(Arc<AtomicUsize>);

impl TextSystem for MeasuredText {
    fn measure(&self, _request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
        None
    }

    fn layout(&self, request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let width = request.text.chars().count() as f32 * 7.0;
        let lines = if request.max_lines.is_none() && request.bounds.width() > 0.0 {
            (width / request.bounds.width()).ceil().max(1.0)
        } else {
            1.0
        };
        Some(TextLayout::new(
            width.min(request.bounds.width()),
            request.font_height * lines,
            false,
            vec![],
            vec![],
            vec![],
        ))
    }
}

fn content_text(value: &str, color: Color) -> UiNode {
    UiNode::new(
        UiId::owned("text"),
        UiNodeKind::Text,
        UiRect::new(0.0, 0.0, 1.0, 1.0),
    )
    .parent(UiId::owned("root"))
    .text(value.to_owned(), TextStyle::new(color, 12.0, 400))
    .size_constraints(SizeConstraints::new(Dimension::Content, Dimension::Content))
}

#[test]
fn intrinsic_text_changes_move_siblings_but_color_changes_do_not_remeasure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let _guard =
        crate::text::install_text_system(TextSystemHandle::new(MeasuredText(calls.clone())));
    let mut tree = HostTree::new();
    tree.push(stack(
        "root",
        UiRect::new(0.0, 0.0, 300.0, 100.0),
        Axis::Horizontal,
        4.0,
        0.0,
        Align::Start,
    ));
    tree.push(content_text("abc", Color::WHITE));
    tree.push(panel("sibling", "root", 20.0, 20.0));
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "text").width(), 21.0);
    assert_eq!(rect(&tree, "sibling").left, 25.0);
    let measured = calls.load(Ordering::SeqCst);

    tree.upsert(content_text("abc", Color(0xFF0000)));
    assert_eq!(runtime.update(&mut tree).laid_out_nodes, 0);
    assert_eq!(calls.load(Ordering::SeqCst), measured);

    tree.upsert(content_text("abcdef", Color::WHITE));
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "text").width(), 42.0);
    assert_eq!(rect(&tree, "sibling").left, 46.0);
}

#[test]
fn definite_width_is_used_to_measure_wrapped_content_height() {
    let calls = Arc::new(AtomicUsize::new(0));
    let _guard = crate::text::install_text_system(TextSystemHandle::new(MeasuredText(calls)));
    let mut tree = HostTree::new();
    tree.push(UiNode::new(
        UiId::owned("root"),
        UiNodeKind::Group,
        UiRect::new(0.0, 0.0, 100.0, 100.0),
    ));
    tree.push(
        content_text("abcdef", Color::WHITE)
            .text(
                "abcdef",
                TextStyle::new(Color::WHITE, 12.0, 400).wrap(crate::core::TextWrap::Words),
            )
            .size_constraints(SizeConstraints::new(
                Dimension::Points(21.0),
                Dimension::Content,
            )),
    );
    apply_layout_tree(&mut tree);
    assert_eq!(rect(&tree, "text").width(), 21.0);
    assert_eq!(rect(&tree, "text").height(), 24.0);
}

#[test]
fn declared_and_minimum_widths_constrain_wrapped_content_measurement() {
    let calls = Arc::new(AtomicUsize::new(0));
    let _guard = crate::text::install_text_system(TextSystemHandle::new(MeasuredText(calls)));
    for (available, sizing) in [
        (
            100.0,
            SizeConstraints::new(Dimension::Auto, Dimension::Content),
        ),
        (
            14.0,
            SizeConstraints::new(Dimension::Content, Dimension::Content)
                .min_size(Size::new(21.0, 0.0)),
        ),
    ] {
        let mut tree = HostTree::new();
        tree.push(UiNode::new(
            UiId::owned("root"),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, available, 100.0),
        ));
        tree.push(
            UiNode::new(
                UiId::owned("text"),
                UiNodeKind::Text,
                UiRect::new(0.0, 0.0, 21.0, 1.0),
            )
            .parent(UiId::owned("root"))
            .text(
                "abcdef",
                TextStyle::new(Color::WHITE, 12.0, 400).wrap(crate::core::TextWrap::Words),
            )
            .size_constraints(sizing),
        );
        apply_layout_tree(&mut tree);
        assert_eq!(rect(&tree, "text").width(), 21.0);
        assert_eq!(rect(&tree, "text").height(), 24.0);
    }
}

#[test]
fn nested_content_size_changes_invalidate_the_outer_layout() {
    let mut tree = HostTree::new();
    tree.push(stack(
        "root",
        UiRect::new(0.0, 0.0, 300.0, 100.0),
        Axis::Horizontal,
        4.0,
        0.0,
        Align::Start,
    ));
    tree.push(
        stack(
            "inner",
            UiRect::new(0.0, 0.0, 1.0, 1.0),
            Axis::Vertical,
            2.0,
            0.0,
            Align::Start,
        )
        .parent(UiId::owned("root"))
        .size_constraints(SizeConstraints::new(Dimension::Content, Dimension::Content)),
    );
    tree.push(panel("first", "inner", 20.0, 10.0));
    tree.push(panel("second", "inner", 30.0, 15.0));
    tree.push(panel("sibling", "root", 20.0, 20.0));
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "inner").width(), 30.0);
    assert_eq!(rect(&tree, "inner").height(), 27.0);
    assert_eq!(rect(&tree, "sibling").left, 34.0);

    tree.upsert(panel("first", "inner", 40.0, 10.0));
    runtime.update(&mut tree);
    assert_eq!(rect(&tree, "inner").width(), 40.0);
    assert_eq!(rect(&tree, "sibling").left, 44.0);
}

#[test]
fn sizing_only_change_is_part_of_the_retained_projection() {
    let mut tree = HostTree::new();
    let make = |width| {
        UiNode::new(
            UiId::owned("root"),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 1.0, 1.0),
        )
        .size_constraints(SizeConstraints::new(
            Dimension::Points(width),
            Dimension::Points(20.0),
        ))
    };
    tree.push(make(40.0));
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut tree);
    tree.upsert(make(80.0));
    assert_eq!(runtime.update(&mut tree).laid_out_nodes, 1);
    assert_eq!(rect(&tree, "root").width(), 80.0);
}

#[test]
fn moving_and_resizing_preserve_paint_outsets_and_translate_descendants() {
    let mut tree = HostTree::new();
    tree.push(stack(
        "root",
        UiRect::new(20.0, 30.0, 220.0, 130.0),
        Axis::Horizontal,
        0.0,
        0.0,
        Align::Start,
    ));
    tree.push(
        panel("child", "root", 40.0, 20.0)
            .paint_bounds(UiRect::new(-2.0, -3.0, 42.0, 23.0))
            .size_constraints(SizeConstraints::new(
                Dimension::Points(80.0),
                Dimension::Points(30.0),
            )),
    );
    tree.push(
        UiNode::new(
            UiId::owned("grandchild"),
            UiNodeKind::Panel,
            UiRect::new(5.0, 5.0, 15.0, 15.0),
        )
        .parent(UiId::owned("child")),
    );
    apply_layout_tree(&mut tree);
    assert_eq!(
        tree.node(&UiId::owned("child")).unwrap().paint_bounds,
        UiRect::new(18.0, 27.0, 102.0, 63.0)
    );
    assert_eq!(
        rect(&tree, "grandchild"),
        UiRect::new(25.0, 35.0, 35.0, 45.0)
    );
}

#[test]
fn declarative_content_strings_use_measured_widths_in_a_session() {
    let calls = Arc::new(AtomicUsize::new(0));
    let _guard = crate::text::install_text_system(TextSystemHandle::new(MeasuredText(calls)));
    let view: crate::application::AppView = Arc::new(|_| crate::core::content_text("abcd"));
    let mut session = crate::session::UiSession::new();
    session.render_view(
        &view,
        UiRect::new(0.0, 0.0, 300.0, 100.0),
        crate::core::UiScale::ONE,
    );
    let text = session
        .tree()
        .nodes()
        .iter()
        .find(|node| node.text.as_deref() == Some("abcd"))
        .unwrap();
    assert_eq!(text.layout_rect.width(), 28.0);
    assert_eq!(text.layout_rect.height(), 18.0);
    assert!(text.size_constraints.is_some());
}
