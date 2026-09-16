use super::*;
use crate::core::{Point, StaticLayerSource, Stroke, UiNodeKind};

fn id(value: &str) -> UiId {
    UiId::owned(value.to_string())
}

fn compositing_layer_tree(layer_rect: UiRect, child_rect: UiRect) -> HostTree {
    compositing_layer_tree_with_spec(layer_rect, child_rect, CompositingLayerSpec::new())
}

fn compositing_layer_tree_with_spec(
    layer_rect: UiRect,
    child_rect: UiRect,
    spec: CompositingLayerSpec,
) -> HostTree {
    let layer_id = id("layer");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(layer_id.clone(), UiNodeKind::CompositingLayer, layer_rect)
            .compositing_layer(spec),
    );
    tree.push(
        UiNode::new(id("layer-child"), UiNodeKind::Panel, child_rect)
            .parent(layer_id)
            .style(VisualStyle::filled(Color::WHITE)),
    );
    tree
}

#[test]
fn compositing_layer_commands_use_layer_local_coordinates() {
    let scene = compile_scene(&compositing_layer_tree(
        UiRect::new(100.0, 200.0, 300.0, 400.0),
        UiRect::new(120.0, 230.0, 180.0, 290.0),
    ));
    let ScenePrimitive::CompositingLayer { rect, commands, .. } = &scene.commands()[0] else {
        panic!("expected compositing layer");
    };
    assert_eq!(*rect, UiRect::new(100.0, 200.0, 300.0, 400.0));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].rect(), UiRect::new(20.0, 30.0, 80.0, 90.0));
}

#[test]
fn moving_a_compositing_layer_preserves_its_content_signature() {
    let first = compile_scene(&compositing_layer_tree(
        UiRect::new(100.0, 200.0, 300.0, 400.0),
        UiRect::new(120.0, 230.0, 180.0, 290.0),
    ));
    let second = compile_scene(&compositing_layer_tree(
        UiRect::new(500.0, 600.0, 700.0, 800.0),
        UiRect::new(520.0, 630.0, 580.0, 690.0),
    ));
    let ScenePrimitive::CompositingLayer {
        content_signature: first_signature,
        ..
    } = &first.commands()[0]
    else {
        panic!("expected first compositing layer");
    };
    let ScenePrimitive::CompositingLayer {
        content_signature: second_signature,
        ..
    } = &second.commands()[0]
    else {
        panic!("expected second compositing layer");
    };
    assert_eq!(first_signature, second_signature);
}

#[test]
fn transforming_a_compositing_layer_preserves_its_content_signature() {
    let rect = UiRect::new(100.0, 200.0, 300.0, 400.0);
    let child = UiRect::new(120.0, 230.0, 180.0, 290.0);
    let first = compile_scene(&compositing_layer_tree_with_spec(
        rect,
        child,
        CompositingLayerSpec::new(),
    ));
    let second = compile_scene(&compositing_layer_tree_with_spec(
        rect,
        child,
        CompositingLayerSpec::new()
            .rotation_degrees(37.0)
            .scale_xy(1.2, 0.8)
            .translation(50.0, -30.0)
            .transform_origin(0.25, 0.75),
    ));
    let ScenePrimitive::CompositingLayer {
        content_signature: first_signature,
        ..
    } = &first.commands()[0]
    else {
        panic!("expected first compositing layer");
    };
    let ScenePrimitive::CompositingLayer {
        content_signature: second_signature,
        ..
    } = &second.commands()[0]
    else {
        panic!("expected second compositing layer");
    };
    assert_eq!(first_signature, second_signature);
    assert_ne!(
        first.commands()[0].signature(),
        second.commands()[0].signature()
    );
}

#[test]
fn physical_projection_preserves_normalized_transform_and_scales_translation() {
    let transform_spec = CompositingLayerSpec::new()
        .rotation_degrees(42.5)
        .scale_xy(1.25, 0.75)
        .translation(100.0, -20.0)
        .transform_origin(0.2, 0.8);
    let scene = compile_scene(&compositing_layer_tree_with_spec(
        UiRect::new(10.0, 20.0, 110.0, 220.0),
        UiRect::new(20.0, 30.0, 50.0, 60.0),
        transform_spec,
    ));
    let projected = scene.project_to_physical(UiScale::new(1.5));
    let ScenePrimitive::CompositingLayer { rect, spec, .. } = &projected.commands()[0] else {
        panic!("expected compositing layer");
    };
    assert_eq!(*rect, UiRect::new(15.0, 30.0, 165.0, 330.0));
    assert_eq!(spec.transform.rotation_degrees_f32(), 42.5);
    assert_eq!(
        (spec.transform.scale_x(), spec.transform.scale_y()),
        (1.25, 0.75)
    );
    assert_eq!(
        (spec.transform.origin_x(), spec.transform.origin_y()),
        (0.2, 0.8)
    );
    assert_eq!(
        (
            spec.transform.translation_x(),
            spec.transform.translation_y()
        ),
        (150.0, -30.0)
    );
}

#[test]
fn transformed_layer_damage_stays_inside_the_parent_layer() {
    let command = |spec| ScenePrimitive::CompositingLayer {
        id: id("animated-layer"),
        rect: UiRect::new(40.0, 40.0, 120.0, 120.0),
        spec,
        commands: Vec::new(),
        content_signature: 7,
        phase: RenderPhase::Content,
    };
    let previous = [command(CompositingLayerSpec::new())];
    let next = [command(
        CompositingLayerSpec::new()
            .rotation_degrees(30.0)
            .scale(1.2),
    )];
    let parent = UiRect::new(0.0, 0.0, 160.0, 160.0);
    let damage = compositing_layer_damage(&previous, &next, parent);
    assert!(!damage.is_empty());
    assert!(damage
        .iter()
        .all(|rect| rect.intersect(parent) == Some(*rect)));
}

#[test]
fn compositing_layer_keeps_its_scene_order_between_siblings() {
    let layer_id = id("ordered-layer");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            id("background"),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .style(VisualStyle::filled(Color::BLACK)),
    );
    tree.push(
        UiNode::new(
            layer_id.clone(),
            UiNodeKind::CompositingLayer,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .compositing_layer(CompositingLayerSpec::new()),
    );
    tree.push(
        UiNode::new(
            id("layer-content"),
            UiNodeKind::Panel,
            UiRect::new(10.0, 10.0, 20.0, 20.0),
        )
        .parent(layer_id)
        .style(VisualStyle::filled(Color::WHITE)),
    );
    tree.push(
        UiNode::new(
            id("foreground"),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .style(VisualStyle::filled(Color::WHITE)),
    );

    let scene = compile_scene(&tree);
    assert_eq!(scene.commands().len(), 3);
    assert_eq!(scene.commands()[0].id().as_str(), "background");
    assert_eq!(scene.commands()[1].id().as_str(), "ordered-layer");
    assert_eq!(scene.commands()[2].id().as_str(), "foreground");
}

#[test]
fn compositing_layer_damage_tracks_moved_inserted_and_removed_commands() {
    let style = VisualStyle::filled(Color::WHITE);
    let previous = vec![
        ScenePrimitive::Rect {
            id: id("moving"),
            rect: UiRect::new(10.0, 10.0, 30.0, 30.0),
            style,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Rect {
            id: id("removed"),
            rect: UiRect::new(60.0, 10.0, 80.0, 30.0),
            style,
            phase: RenderPhase::Content,
        },
    ];
    let next = vec![
        ScenePrimitive::Rect {
            id: id("moving"),
            rect: UiRect::new(20.0, 50.0, 40.0, 70.0),
            style,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Rect {
            id: id("inserted"),
            rect: UiRect::new(70.0, 60.0, 90.0, 80.0),
            style,
            phase: RenderPhase::Content,
        },
    ];

    let damage = compositing_layer_damage(&previous, &next, UiRect::new(0.0, 0.0, 100.0, 100.0));
    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(15.0, 15.0))));
    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(25.0, 55.0))));
    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(65.0, 15.0))));
    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(75.0, 65.0))));
}

#[test]
fn nested_compositing_layer_damage_stays_local_to_changed_content() {
    let nested = |child_rect| {
        let child = ScenePrimitive::Rect {
            id: id("nested-child"),
            rect: child_rect,
            style: VisualStyle::filled(Color::WHITE),
            phase: RenderPhase::Content,
        };
        ScenePrimitive::CompositingLayer {
            id: id("nested"),
            rect: UiRect::new(100.0, 80.0, 300.0, 280.0),
            spec: CompositingLayerSpec::new(),
            content_signature: child.signature(),
            commands: vec![child],
            phase: RenderPhase::Content,
        }
    };
    let damage = compositing_layer_damage(
        &[nested(UiRect::new(10.0, 10.0, 30.0, 30.0))],
        &[nested(UiRect::new(20.0, 20.0, 40.0, 40.0))],
        UiRect::new(0.0, 0.0, 500.0, 400.0),
    );

    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(115.0, 95.0))));
    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(125.0, 105.0))));
    assert!(damage
        .iter()
        .all(|rect| rect.right < 200.0 && rect.bottom < 180.0));
}

#[test]
fn popup_escapes_a_regular_compositing_layer() {
    let layer_id = id("layer");
    let content_id = id("content");
    let popup_id = id("popup");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            layer_id.clone(),
            UiNodeKind::CompositingLayer,
            UiRect::new(100.0, 100.0, 300.0, 300.0),
        )
        .compositing_layer(CompositingLayerSpec::new()),
    );
    tree.push(
        UiNode::new(
            content_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(120.0, 130.0, 180.0, 190.0),
        )
        .parent(layer_id.clone())
        .style(VisualStyle::filled(Color::WHITE)),
    );
    tree.push(
        UiNode::new(
            popup_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(140.0, 150.0, 240.0, 250.0),
        )
        .parent(layer_id)
        .style(VisualStyle::filled(Color::WHITE))
        .render_phase(RenderPhase::Popup),
    );

    let scene = compile_scene(&tree);
    assert_eq!(scene.commands().len(), 2);
    let ScenePrimitive::CompositingLayer { commands, .. } = &scene.commands()[0] else {
        panic!("expected compositing layer");
    };
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id(), &content_id);
    assert_eq!(commands[0].rect(), UiRect::new(20.0, 30.0, 80.0, 90.0));
    assert_eq!(scene.commands()[1].id(), &popup_id);
}

#[test]
fn popup_commands_escape_clips_and_render_after_regular_commands() {
    let clip_id = id("clip");
    let content_id = id("content");
    let popup_id = id("popup");
    let overlay_id = id("overlay");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            clip_id.clone(),
            UiNodeKind::Clip,
            UiRect::new(0.0, 0.0, 20.0, 20.0),
        )
        .clip(UiRect::new(0.0, 0.0, 20.0, 20.0), 0.0, -12.0),
    );
    tree.push(
        UiNode::new(
            content_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 10.0, 10.0),
        )
        .parent(clip_id.clone())
        .style(VisualStyle::filled(Color::WHITE)),
    );
    tree.push(
        UiNode::new(
            popup_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(20.0, 20.0, 40.0, 40.0),
        )
        .parent(clip_id)
        .style(VisualStyle::filled(Color::WHITE))
        .render_phase(RenderPhase::Popup),
    );
    tree.push(
        UiNode::new(
            overlay_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(20.0, 20.0, 40.0, 40.0),
        )
        .style(VisualStyle::filled(Color::BLACK))
        .render_phase(RenderPhase::Overlay),
    );

    let list = compile_scene(&tree);
    assert_eq!(list.commands().len(), 3);
    assert_eq!(list.commands()[0].id(), &id("clip"));
    assert_eq!(list.commands()[1].id(), &overlay_id);
    assert_eq!(list.commands()[2].id(), &popup_id);
    assert_eq!(
        list.commands()[2].rect(),
        UiRect::new(20.0, 8.0, 40.0, 28.0)
    );
    let ScenePrimitive::Clip { commands, .. } = &list.commands()[0] else {
        panic!("expected clip command");
    };
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id(), &content_id);
}

#[test]
fn popup_clip_keeps_its_children_nested() {
    let popup_clip_id = id("popup-clip");
    let popup_child_id = id("popup-child");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            popup_clip_id.clone(),
            UiNodeKind::Clip,
            UiRect::new(10.0, 10.0, 50.0, 50.0),
        )
        .clip(UiRect::new(10.0, 10.0, 50.0, 50.0), 0.0, 0.0)
        .render_phase(RenderPhase::Popup),
    );
    tree.push(
        UiNode::new(
            popup_child_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(12.0, 12.0, 48.0, 48.0),
        )
        .parent(popup_clip_id.clone())
        .style(VisualStyle::filled(Color::WHITE))
        .render_phase(RenderPhase::Popup),
    );

    let list = compile_scene(&tree);
    assert_eq!(list.commands().len(), 1);
    let ScenePrimitive::Clip { commands, .. } = &list.commands()[0] else {
        panic!("expected popup clip command");
    };
    assert_eq!(list.commands()[0].id(), &popup_clip_id);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id(), &popup_child_id);
}

#[test]
fn popup_static_layer_translates_its_nested_commands_with_ancestor_content() {
    let ancestor_id = id("ancestor");
    let popup_layer_id = id("popup-layer");
    let popup_child_id = id("popup-layer-child");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            ancestor_id.clone(),
            UiNodeKind::Clip,
            UiRect::new(0.0, 0.0, 60.0, 60.0),
        )
        .clip(UiRect::new(0.0, 0.0, 60.0, 60.0), 0.0, -12.0),
    );
    tree.push(
        UiNode::new(
            popup_layer_id.clone(),
            UiNodeKind::StaticLayer,
            UiRect::new(20.0, 20.0, 40.0, 40.0),
        )
        .parent(ancestor_id)
        .static_layer(StaticLayerSpec::new(StaticLayerSource::runtime()))
        .render_phase(RenderPhase::Popup),
    );
    tree.push(
        UiNode::new(
            popup_child_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(22.0, 22.0, 38.0, 38.0),
        )
        .parent(popup_layer_id.clone())
        .style(VisualStyle::filled(Color::WHITE))
        .render_phase(RenderPhase::Popup),
    );

    let list = compile_scene(&tree);
    assert_eq!(list.commands().len(), 2);
    let ScenePrimitive::StaticLayer { rect, commands, .. } = &list.commands()[1] else {
        panic!("expected popup static layer command");
    };
    assert_eq!(list.commands()[1].id(), &popup_layer_id);
    assert_eq!(*rect, UiRect::new(20.0, 8.0, 40.0, 28.0));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id(), &popup_child_id);
    assert_eq!(commands[0].rect(), UiRect::new(22.0, 10.0, 38.0, 26.0));
}

#[test]
fn physical_projection_scales_nested_raster_commands_and_cache_keys() {
    let text = ScenePrimitive::Text {
        id: id("text"),
        rect: UiRect::new(2.0, 4.0, 12.0, 14.0),
        text: Cow::Borrowed("DPI"),
        style: TextStyle::new(Color::WHITE, -11.0, 700).tracking(2.0),
        phase: RenderPhase::Content,
    };
    let clip = ScenePrimitive::Clip {
        id: id("clip"),
        rect: UiRect::new(1.0, 1.0, 3.0, 3.0),
        commands: vec![text],
        child_signature: 5,
        phase: RenderPhase::Content,
    };
    let list = Scene {
        commands: Arc::new(vec![ScenePrimitive::ScrollRaster {
            id: id("raster"),
            viewport: UiRect::new(0.0, 0.0, 100.0, 80.0),
            spec: ScrollRasterSpec {
                cache_epoch: 11,
                content_height: 200.0,
                scroll_y: 4.0,
                tile_height_px: 32.0,
                memory_budget_bytes: 1024,
                background_fill: Some(Color::BLACK),
                visible_tiles: vec![0, 1],
                prefetch_tiles: vec![2],
                max_prefetch_tiles_per_frame: 1,
                max_prefetch_ms_per_frame: 2,
            },
            commands: vec![clip],
            child_signature: 7,
            phase: RenderPhase::Content,
        }]),
    };

    let projected = list.project_to_physical(UiScale::new(1.5));
    let ScenePrimitive::ScrollRaster {
        viewport,
        spec,
        commands,
        child_signature,
        ..
    } = &projected.commands[0]
    else {
        panic!("expected scroll raster");
    };
    assert_eq!(*viewport, UiRect::new(0.0, 0.0, 150.0, 120.0));
    assert_eq!(spec.content_height, 300.0);
    assert_eq!(spec.scroll_y, 6.0);
    assert_eq!(spec.tile_height_px, 48.0);
    assert_ne!(spec.cache_epoch, 11);
    assert_ne!(*child_signature, 7);

    let ScenePrimitive::Clip {
        rect,
        commands,
        child_signature,
        ..
    } = &commands[0]
    else {
        panic!("expected clip");
    };
    assert_eq!(*rect, UiRect::new(1.5, 1.5, 4.5, 4.5));
    assert_ne!(*child_signature, 5);

    let ScenePrimitive::Text { rect, style, .. } = &commands[0] else {
        panic!("expected text");
    };
    assert_eq!(*rect, UiRect::new(3.0, 6.0, 18.0, 21.0));
    assert_eq!(style.height, -16.5);
    assert_eq!(style.tracking, 3.0);
}

#[test]
fn cloned_scenes_share_command_storage_until_mutated() {
    let mut scene = Scene::new();
    let cloned = scene.clone();
    assert!(scene.shares_command_storage_with(&cloned));

    scene.push(ScenePrimitive::Rect {
        id: id("changed"),
        rect: UiRect::new(0.0, 0.0, 1.0, 1.0),
        style: VisualStyle::filled(Color::WHITE),
        phase: RenderPhase::Content,
    });
    assert!(!scene.shares_command_storage_with(&cloned));
}

#[test]
fn backend_translation_preserves_nested_static_layer_local_commands() {
    let command = ScenePrimitive::StaticLayer {
        id: id("static"),
        rect: UiRect::new(10.0, 20.0, 110.0, 120.0),
        spec: StaticLayerSpec::new(StaticLayerSource::runtime()),
        commands: vec![ScenePrimitive::Rect {
            id: id("child"),
            rect: UiRect::new(2.0, 3.0, 12.0, 13.0),
            style: VisualStyle::filled(Color::WHITE),
            phase: RenderPhase::Content,
        }],
        child_signature: 7,
        phase: RenderPhase::Content,
    };

    let translated = translate_scene_primitive_for_backend(&command, 5.0, 6.0);
    let ScenePrimitive::StaticLayer { rect, commands, .. } = translated else {
        panic!("expected static layer");
    };
    assert_eq!(rect, UiRect::new(15.0, 26.0, 115.0, 126.0));
    assert_eq!(commands[0].rect(), UiRect::new(2.0, 3.0, 12.0, 13.0));
}

#[test]
fn physical_projection_scales_compositing_bounds_and_local_commands() {
    let child = ScenePrimitive::Rect {
        id: id("layer-child"),
        rect: UiRect::new(2.0, 4.0, 12.0, 14.0),
        style: VisualStyle::filled(Color::WHITE),
        phase: RenderPhase::Content,
    };
    let content_signature = child.signature();
    let scene = Scene {
        commands: Arc::new(vec![ScenePrimitive::CompositingLayer {
            id: id("layer"),
            rect: UiRect::new(10.0, 20.0, 110.0, 120.0),
            spec: CompositingLayerSpec::new(),
            commands: vec![child],
            content_signature,
            phase: RenderPhase::Content,
        }]),
    };

    let projected = scene.project_to_physical(UiScale::new(1.5));
    let ScenePrimitive::CompositingLayer {
        rect,
        commands,
        content_signature: projected_signature,
        ..
    } = &projected.commands()[0]
    else {
        panic!("expected compositing layer");
    };
    assert_eq!(*rect, UiRect::new(15.0, 30.0, 165.0, 180.0));
    assert_eq!(commands[0].rect(), UiRect::new(3.0, 6.0, 18.0, 21.0));
    assert_ne!(*projected_signature, content_signature);
}

#[test]
fn physical_projection_scales_paths_strokes_radii_and_static_offsets() {
    let path = UiPath::new([
        UiPathCommand::MoveTo(Point::new(2.0, 3.0)),
        UiPathCommand::QuadraticTo {
            control: Point::new(4.0, 5.0),
            to: Point::new(6.0, 7.0),
        },
        UiPathCommand::Close,
    ]);
    let list = Scene {
        commands: Arc::new(vec![
            ScenePrimitive::Rect {
                id: id("rect"),
                rect: UiRect::new(1.0, 2.0, 11.0, 12.0),
                style: VisualStyle::filled(Color::WHITE)
                    .radius(3.0)
                    .stroked(Stroke::new(Color::BLACK, 2.0, 255)),
                phase: RenderPhase::Background,
            },
            ScenePrimitive::Path {
                id: id("path"),
                rect: UiRect::new(0.0, 0.0, 8.0, 8.0),
                path,
                style: PathStyle {
                    fill: None,
                    fill_alpha: 255,
                    stroke: Some(Stroke::new(Color::WHITE, 2.0, 255)),
                },
                phase: RenderPhase::Content,
            },
            ScenePrimitive::StaticLayer {
                id: id("static"),
                rect: UiRect::new(0.0, 0.0, 10.0, 10.0),
                spec: StaticLayerSpec::new(StaticLayerSource::runtime()).paint_offset(-2.0, 3.0),
                commands: Vec::new(),
                child_signature: 13,
                phase: RenderPhase::Background,
            },
        ]),
    };

    let projected = list.project_to_physical(UiScale::new(1.25));
    let ScenePrimitive::Rect { style, .. } = &projected.commands[0] else {
        panic!("expected rect");
    };
    assert_eq!(style.radius, 3.75);
    assert_eq!(style.stroke.expect("stroke").width, 2.5);

    let ScenePrimitive::Path { path, style, .. } = &projected.commands[1] else {
        panic!("expected path");
    };
    assert_eq!(
        path.commands(),
        &[
            UiPathCommand::MoveTo(Point::new(2.5, 3.75)),
            UiPathCommand::QuadraticTo {
                control: Point::new(5.0, 6.25),
                to: Point::new(7.5, 8.75),
            },
            UiPathCommand::Close,
        ]
    );
    assert_eq!(style.stroke.expect("stroke").width, 2.5);

    let ScenePrimitive::StaticLayer {
        spec,
        child_signature,
        ..
    } = &projected.commands[2]
    else {
        panic!("expected static layer");
    };
    assert_eq!((spec.offset_x, spec.offset_y), (-2.5, 3.75));
    assert_ne!(*child_signature, 13);
}
