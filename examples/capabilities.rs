//! Independent capability gallery. No Loom or extension dependency.
use lgui::prelude::*;
use lgui::{
    WinitApplication,
    core::{SemanticRole, Semantics},
};
use std::{collections::BTreeSet, sync::Arc};

fn label(rect: UiRect, value: impl Into<String>) -> Element {
    text(
        rect,
        value.into(),
        TextStyle::new(Color(0xDFE7F1), 14.0, 400),
    )
    .into()
}
fn app(cx: &mut RenderCx<'_, '_>) -> Element {
    let input = cx.state(TextInputModel::new("Edit me — 中文 / 👩‍💻"));
    let multiline = cx.state(TextInputModel::new(
        "Portable multiline input\nSelection, IME, undo and scrolling",
    ));
    let ratio = cx.state(0.42);
    let offset = cx.state(0.0);
    let variable_offset = cx.state(0.0);
    let expanded = cx.state(BTreeSet::new());
    let selected = cx.state(None);
    let tree_offset = cx.state(0.0);
    let tab = cx.state(0usize);
    let visible = cx.state(true);
    let toggle = visible.clone();
    let close = visible.clone();
    let outside = visible.clone();
    let area = UiRect::new(0.0, 0.0, 960.0, 720.0);
    let mut root = group(area).public_id("gallery").layout(LayoutSpec::Dock {
        padding: EdgeInsets::all(16.0),
        gap: Dimension::Points(12.0),
    });
    root = root.child(
        label(
            UiRect::new(0.0, 0.0, 928.0, 32.0),
            "LGUI • independent capability gallery",
        )
        .dock(DockSide::Top),
    );
    let footer = flex(UiRect::new(0.0, 0.0, 928.0, 32.0), Axis::Horizontal)
        .gap(10.0)
        .align(Align::Center)
        .content((
            badge(UiRect::new(0.0, 0.0, 120.0, 28.0), "Status: ready", None),
            progress_ring(
                UiRect::new(0.0, 0.0, 28.0, 28.0),
                Some(0.7),
                Color(0x58CCAE),
            ),
            progress_ring(UiRect::new(0.0, 0.0, 28.0, 28.0), None, Color(0x72B5FF)),
            icon_button(
                UiRect::new(0.0, 0.0, 36.0, 28.0),
                String::from("copy"),
                "Toggle floating content",
            )
            .public_id("toggle")
            .on_click(move || toggle.update(|v| *v = !*v)),
            label(
                UiRect::new(0.0, 0.0, 460.0, 28.0),
                "Drag splitter · scroll lists · expand tree · switch tabs",
            ),
        ));
    root = root.child(Element::from(footer).dock(DockSide::Bottom));
    let body = split(
        UiRect::new(16.0, 60.0, 944.0, 660.0),
        Axis::Horizontal,
        ratio,
        (260.0, 300.0),
        move |r| {
            let mut grid = GridLayout::new(vec![GridTrack::Fraction(1.0)]).rows(vec![
                GridTrack::Points(40.0),
                GridTrack::Points(116.0),
                GridTrack::Fraction(1.0),
            ]);
            grid.gap = Size::new(8.0, 8.0);
            group(r).layout(LayoutSpec::Grid(grid)).children(vec![
                Element::from(text_input(
                    UiRect::new(r.left, r.top, r.right, r.top + 40.0),
                    input,
                ))
                .public_id("input"),
                Element::from(multiline_input(
                    UiRect::new(r.left, r.top + 48.0, r.right, r.top + 164.0),
                    multiline,
                ))
                .public_id("multiline"),
                tabs(
                    UiRect::new(r.left, r.top + 172.0, r.right, r.bottom),
                    vec!["Tree".into(), "Variable list".into()].into(),
                    tab,
                    move |i, body| {
                        if i == 0 {
                            tree_view(
                                body,
                                vec![
                                    TreeNode::new("src", "src")
                                        .child(TreeNode::new("main", "main.rs"))
                                        .child(TreeNode::new("lib", "lib.rs")),
                                    TreeNode::new("docs", "docs")
                                        .child(TreeNode::new("api", "API.md")),
                                ]
                                .into(),
                                expanded,
                                selected,
                                tree_offset,
                            )
                        } else {
                            virtual_list(
                                body,
                                Arc::new(RowHeights::variable(
                                    (0..10_000).map(|i| if i % 3 == 0 { 42.0 } else { 24.0 }),
                                )),
                                variable_offset,
                                |i, row| label(row, format!("Variable row {i}")),
                            )
                        }
                    },
                )
                .public_id("tabs"),
            ])
        },
        move |r| {
            virtual_list(
                r,
                Arc::new(RowHeights::uniform(1_000_000, 24.0)),
                offset,
                |i, row| label(row, format!("Visible row {i} / 1,000,000")),
            )
        },
    );
    root = root.child(body.dock(DockSide::Fill));
    let overlay = presence(
        UiRect::new(0.0, 0.0, 280.0, 92.0),
        visible.get(),
        VisibilityTransition::default(),
        group(UiRect::new(0.0, 0.0, 280.0, 92.0))
            .semantics(Semantics::new(SemanticRole::Group).name("Floating status content"))
            .child(badge(
                UiRect::new(8.0, 8.0, 272.0, 40.0),
                "Anchor / flip / transition",
                Some(Color(0x58CCAE)),
            ))
            .child(label(
                UiRect::new(8.0, 44.0, 272.0, 84.0),
                "Escape or click outside to close",
            )),
    )
    .public_id("floating")
    .position(Position::anchor("toggle", AnchorSide::Top))
    .z_index(10)
    .focus_scope()
    .on_close_requested(move |_| close.set(false))
    .on_outside_pointer(move |_, _, _| outside.set(false));
    root.child(overlay)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::with_backend(WinitApplication::new(GraphicsPreference::Auto))
        .memory_options(MemoryOptions::unbounded(
            ImageCachePolicy::WhileVisible,
            false,
        ))
        .provide(RendererKind::Skia(GraphicsPreference::Auto))
        .window_options(
            WindowOptions::new("capabilities")
                .title("LGUI capabilities")
                .size(Size::new(960.0, 720.0)),
        )
        .run(app)?;
    Ok(())
}
