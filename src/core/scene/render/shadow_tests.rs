use super::*;
use crate::core::ShadowStyle;

fn tree(x: f32, shadow: Option<ShadowStyle>) -> HostTree {
    let mut tree = HostTree::new();
    let mut root = UiNode::new(
        UiId::new("shadow"),
        UiNodeKind::Group,
        UiRect::new(x, 40.0, x + 40.0, 80.0),
    );
    root.shadow = shadow;
    tree.push(root);
    tree.push(
        UiNode::new(
            UiId::new("circle"),
            UiNodeKind::Ellipse,
            UiRect::new(x + 5.0, 45.0, x + 35.0, 75.0),
        )
        .parent(UiId::new("shadow"))
        .style(VisualStyle::filled(Color::WHITE)),
    );
    tree
}

#[test]
fn shadow_is_one_alpha_layer_and_does_not_change_layout_or_hit_bounds() {
    let tree = tree(40.0, Some(ShadowStyle::default()));
    let node = tree.node(&UiId::new("shadow")).unwrap();
    assert_eq!(node.layout_rect, UiRect::new(40.0, 40.0, 80.0, 80.0));
    assert_eq!(node.hit_rect, node.layout_rect);
    let scene = compile_scene(&tree);
    assert_eq!(scene.commands().len(), 1);
    let ScenePrimitive::CompositingLayer {
        rect,
        spec,
        commands,
        ..
    } = &scene.commands()[0]
    else {
        panic!("shadow layer");
    };
    assert!(rect.left < 40.0 && rect.right > 80.0 && rect.bottom > 80.0);
    assert_eq!(spec.shadow, Some(ShadowStyle::default()));
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], ScenePrimitive::Ellipse { .. }));
    assert_eq!(
        commands[0].rect().translate(rect.left, rect.top),
        tree.node(&UiId::new("circle")).unwrap().layout_rect
    );
    assert_eq!(scene_root_ids(&tree), vec![UiId::new("shadow")]);
    assert_eq!(
        compile_scene_root(&tree, &UiId::new("shadow")).commands(),
        scene.commands()
    );
}

#[test]
fn no_shadow_keeps_existing_scene_and_movement_reuses_shadow_content() {
    assert!(matches!(
        compile_scene(&tree(40.0, None)).commands()[0],
        ScenePrimitive::Ellipse { .. }
    ));
    let a = compile_scene(&tree(40.0, Some(ShadowStyle::default())));
    let b = compile_scene(&tree(100.0, Some(ShadowStyle::default())));
    let signature = |scene: &Scene| match scene.commands()[0] {
        ScenePrimitive::CompositingLayer {
            content_signature, ..
        } => content_signature,
        _ => panic!("layer"),
    };
    assert_eq!(signature(&a), signature(&b));
}

#[test]
fn shadow_scales_filter_distances_with_dpi() {
    let style = ShadowStyle::default()
        .offset(-2.0, 4.0)
        .blur(3.0)
        .spread(2.0);
    let logical = compile_scene(&tree(40.0, Some(style)));
    let physical = logical.project_to_physical(UiScale::new(1.5));
    let ScenePrimitive::CompositingLayer { spec, rect, .. } = physical.commands()[0] else {
        panic!("layer");
    };
    let projected = spec.shadow.unwrap();
    assert_eq!(
        (
            projected.offset_x(),
            projected.offset_y(),
            projected.blur_sigma(),
            projected.spread_radius()
        ),
        (-3.0, 6.0, 4.5, 3.0)
    );
    let expected = UiScale::new(1.5).physical_ui_rect(logical.commands()[0].rect());
    assert_eq!(
        rect,
        UiRect::new(
            expected.left.floor(),
            expected.top.floor(),
            expected.right.ceil(),
            expected.bottom.ceil()
        )
    );
}

#[test]
fn shadow_respects_internal_clip_and_excludes_detached_popups() {
    let mut tree = tree(40.0, Some(ShadowStyle::default()));
    tree.push(
        UiNode::new(
            UiId::new("popup"),
            UiNodeKind::Panel,
            UiRect::new(200.0, 200.0, 250.0, 250.0),
        )
        .parent(UiId::new("shadow"))
        .style(VisualStyle::filled(Color::WHITE))
        .render_phase(RenderPhase::Popup),
    );
    let scene = compile_scene(&tree);
    assert_eq!(scene.commands().len(), 2);
    assert_eq!(scene.commands()[1].id(), &UiId::new("popup"));
    assert!(scene.commands()[0].paint_bounds().right < 200.0);

    let mut clipped = HostTree::new();
    let bounds = UiRect::new(40.0, 40.0, 80.0, 80.0);
    clipped.push(
        UiNode::new(UiId::new("clip"), UiNodeKind::Clip, bounds).shadow(ShadowStyle::default()),
    );
    clipped.push(
        UiNode::new(
            UiId::new("wide"),
            UiNodeKind::Ellipse,
            bounds.inflate(40.0, 40.0),
        )
        .parent(UiId::new("clip"))
        .style(VisualStyle::filled(Color::WHITE)),
    );
    let scene = compile_scene(&clipped);
    let ScenePrimitive::CompositingLayer { commands, .. } = &scene.commands()[0] else {
        panic!("layer");
    };
    assert!(matches!(&commands[0], ScenePrimitive::Clip { commands, .. } if commands.len() == 1));
    assert!(scene.commands()[0].paint_bounds().right < 110.0);
}

#[test]
fn shadow_captures_overflow_and_nested_layers_without_cache_identity_collision() {
    let mut tree = HostTree::new();
    let bounds = UiRect::new(40.0, 40.0, 80.0, 80.0);
    tree.push(
        UiNode::new(UiId::new("composite"), UiNodeKind::CompositingLayer, bounds)
            .compositing_layer(CompositingLayerSpec::new().translation(70.0, 0.0))
            .shadow(ShadowStyle::default()),
    );
    tree.push(
        UiNode::new(UiId::new("shape"), UiNodeKind::Ellipse, bounds)
            .parent(UiId::new("composite"))
            .style(VisualStyle::filled(Color::WHITE)),
    );
    let scene = compile_scene(&tree);
    let ScenePrimitive::CompositingLayer {
        id, commands, rect, ..
    } = &scene.commands()[0]
    else {
        panic!("layer");
    };
    assert_ne!(id, commands[0].id());
    assert!(rect.right > 150.0);
}

#[test]
fn nested_shadow_damage_includes_blur_and_offset() {
    let first = compile_scene(&tree(40.0, Some(ShadowStyle::default())));
    let mut changed = tree(40.0, Some(ShadowStyle::default()));
    changed.push(
        UiNode::new(
            UiId::new("extra"),
            UiNodeKind::Panel,
            UiRect::new(45.0, 45.0, 50.0, 50.0),
        )
        .parent(UiId::new("shadow"))
        .style(VisualStyle::filled(Color::BLACK)),
    );
    let second = compile_scene(&changed);
    let damage = compositing_layer_damage(
        first.commands(),
        second.commands(),
        UiRect::new(0.0, 0.0, 500.0, 500.0),
    );
    let bounds = damage.into_iter().reduce(UiRect::union).unwrap();
    assert!(bounds.left <= first.commands()[0].rect().left);
    assert!(bounds.bottom >= second.commands()[0].rect().bottom);
}
