use super::*;
use crate::core::{CompositingLayerSpec, Point, SemanticRole, Semantics, UiNode, VisualStyle};

fn compositing_content_signature(scene: &Scene, id: &UiId) -> u64 {
    scene
        .commands()
        .iter()
        .find_map(|command| match command {
            ScenePrimitive::CompositingLayer {
                id: command_id,
                content_signature,
                ..
            } if command_id == id => Some(*content_signature),
            _ => None,
        })
        .expect("compositing layer command")
}

fn compositing_child_storage(scene: &Scene, id: &UiId) -> usize {
    scene
        .commands()
        .iter()
        .find_map(|command| match command {
            ScenePrimitive::CompositingLayer {
                id: command_id,
                commands,
                ..
            } if command_id == id => Some(commands.as_ptr() as usize),
            _ => None,
        })
        .expect("compositing layer command")
}

#[test]
fn semantics_only_updates_do_not_damage_visual_content() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 200.0, 100.0);
    let id = UiId::owned("semantic-only");
    let tree = |name: &str| {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                id.clone(),
                UiNodeKind::Button,
                UiRect::new(10.0, 10.0, 90.0, 40.0),
            )
            .style(VisualStyle::filled(crate::core::Color::WHITE))
            .semantics(Semantics::new(SemanticRole::Button).name(name)),
        );
        tree
    };
    host.commit(
        &tree("Before"),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let commit = host.commit(
        &tree("After"),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );

    assert!(commit.damage.dirty.is_empty());
    assert_eq!(commit.semantics.nodes.len(), 1);
    assert_eq!(
        commit.semantics.nodes[0].semantics.name.as_deref(),
        Some("After")
    );
}

#[test]
fn moving_a_node_damages_old_and_new_bounds_without_a_frame_snapshot() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 500.0, 500.0);
    let id = UiId::owned("moving");
    let mut first = HostTree::new();
    first.push(
        UiNode::new(
            id.clone(),
            UiNodeKind::Panel,
            UiRect::new(10.0, 10.0, 40.0, 40.0),
        )
        .style(VisualStyle::default()),
    );
    host.commit(&first, &interaction, viewport, &mut InvalidationSet::new());

    let mut second = HostTree::new();
    second.push(UiNode::new(
        id,
        UiNodeKind::Panel,
        UiRect::new(100.0, 100.0, 130.0, 130.0),
    ));
    let commit = host.commit(&second, &interaction, viewport, &mut InvalidationSet::new());

    assert!(!commit.damage.dirty.is_empty());
    assert!(commit.mutations.iter().any(|mutation| matches!(
        mutation,
        HostMutation::UpdateProps {
            kind: HostUpdateKind::Layout,
            ..
        }
    )));
}

#[test]
fn moving_layer_damages_old_and_new_bounds_without_recompiling_siblings() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 400.0, 300.0);
    let layer_id = UiId::owned("moving-layer");
    let child_id = UiId::owned("moving-layer-child");
    let make_tree = |layer_rect: UiRect, child_rect: UiRect| {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(UiId::owned("background"), UiNodeKind::Panel, viewport)
                .style(VisualStyle::filled(crate::core::Color::BLACK)),
        );
        tree.push(
            UiNode::new(layer_id.clone(), UiNodeKind::CompositingLayer, layer_rect)
                .compositing_layer(CompositingLayerSpec::new()),
        );
        tree.push(
            UiNode::new(child_id.clone(), UiNodeKind::Panel, child_rect)
                .parent(layer_id.clone())
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        tree.push(
            UiNode::new(UiId::owned("foreground"), UiNodeKind::Panel, viewport)
                .style(VisualStyle::default()),
        );
        tree
    };

    host.commit(
        &make_tree(
            UiRect::new(10.0, 20.0, 90.0, 100.0),
            UiRect::new(20.0, 30.0, 40.0, 50.0),
        ),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let commit = host.commit(
        &make_tree(
            UiRect::new(200.0, 160.0, 280.0, 240.0),
            UiRect::new(210.0, 170.0, 230.0, 190.0),
        ),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );

    let dirty = commit.damage.dirty.effective_rects();
    assert!(dirty
        .iter()
        .any(|rect| rect.contains(Point::new(20.0, 30.0))));
    assert!(dirty
        .iter()
        .any(|rect| rect.contains(Point::new(210.0, 170.0))));
    assert!(
        !dirty
            .iter()
            .any(|rect| rect.contains(Point::new(150.0, 130.0))),
        "moving a retained layer must not dirty the area between old and new bounds"
    );
    assert_eq!(commit.metrics.reused_scene_nodes, 2);
    assert_eq!(
        commit
            .scene_mutations
            .iter()
            .filter(|mutation| matches!(mutation, SceneMutation::Update(_)))
            .count(),
        1
    );
}

#[test]
fn shrinking_a_transformed_layer_damages_pixels_outside_its_layout_rect() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 200.0, 200.0);
    let make_tree = |scale| {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                UiId::owned("transformed-layer"),
                UiNodeKind::CompositingLayer,
                UiRect::new(40.0, 40.0, 120.0, 120.0),
            )
            .compositing_layer(CompositingLayerSpec::new().scale(scale)),
        );
        tree.push(
            UiNode::new(
                UiId::owned("transformed-child"),
                UiNodeKind::Panel,
                UiRect::new(50.0, 50.0, 110.0, 110.0),
            )
            .parent(UiId::owned("transformed-layer"))
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        tree
    };

    host.commit(
        &make_tree(1.2),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let commit = host.commit(
        &make_tree(1.0),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );

    let dirty = commit.damage.dirty.effective_rects();
    assert!(dirty
        .iter()
        .any(|rect| rect.contains(Point::new(33.0, 80.0))));
    assert!(dirty
        .iter()
        .any(|rect| rect.contains(Point::new(80.0, 80.0))));
}

#[test]
fn composition_only_update_patches_the_retained_scene_without_compiling_children() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 200.0, 200.0);
    let layer_id = UiId::owned("retained-transform-layer");
    let child_id = UiId::owned("retained-transform-child");
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            layer_id.clone(),
            UiNodeKind::CompositingLayer,
            UiRect::new(0.0, 0.0, 40.0, 40.0),
        )
        .compositing_layer(CompositingLayerSpec::new()),
    );
    tree.push(
        UiNode::new(
            child_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(5.0, 5.0, 35.0, 35.0),
        )
        .parent(layer_id.clone())
        .style(VisualStyle::filled(crate::core::Color::WHITE)),
    );
    let changes = tree.take_projection_changes();
    let first = host.commit_projection(
        &tree,
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
        changes,
    );
    let first_signature = compositing_content_signature(&first.scene, &layer_id);
    let first_child_storage = compositing_child_storage(&first.scene, &layer_id);
    drop(first);

    tree.node_mut(&child_id).expect("child").style =
        VisualStyle::filled(crate::core::Color(0xFF0000));
    assert!(tree
        .update_compositing_layer(
            &layer_id,
            CompositingLayerSpec::new().translation(50.0, 25.0),
        )
        .is_some());
    let changes = tree.take_projection_changes();
    assert_eq!(changes.changed.len(), 1);
    let second = host.commit_projection(
        &tree,
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
        changes,
    );

    assert_eq!(second.metrics.visited_host_nodes, 1);
    assert_eq!(second.metrics.compiled_scene_nodes, 0);

    assert_eq!(
        compositing_content_signature(&second.scene, &layer_id),
        first_signature
    );
    assert_eq!(
        compositing_child_storage(&second.scene, &layer_id),
        first_child_storage,
        "composition-only updates must retain static child command storage"
    );
    let ScenePrimitive::CompositingLayer { spec, .. } = second
        .scene
        .commands()
        .iter()
        .find(|command| command.id() == &layer_id)
        .expect("compositing command")
    else {
        panic!("expected compositing command");
    };
    assert_eq!(
        (
            spec.transform.translation_x(),
            spec.transform.translation_y()
        ),
        (50.0, 25.0)
    );
}

#[test]
fn inserting_content_into_a_large_layer_keeps_window_damage_local() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 800.0, 600.0);
    let layer_id = UiId::owned("large-layer");
    let make_tree = |include_inserted: bool| {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(layer_id.clone(), UiNodeKind::CompositingLayer, viewport)
                .compositing_layer(CompositingLayerSpec::new()),
        );
        tree.push(
            UiNode::new(
                UiId::owned("stable-child"),
                UiNodeKind::Panel,
                UiRect::new(20.0, 20.0, 80.0, 80.0),
            )
            .parent(layer_id.clone())
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        if include_inserted {
            tree.push(
                UiNode::new(
                    UiId::owned("inserted-child"),
                    UiNodeKind::Panel,
                    UiRect::new(120.0, 100.0, 180.0, 160.0),
                )
                .parent(layer_id.clone())
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
            );
        }
        tree
    };

    host.commit(
        &make_tree(false),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let commit = host.commit(
        &make_tree(true),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );

    let damage = commit.damage.dirty.effective_rects();
    assert!(damage
        .iter()
        .any(|rect| rect.contains(Point::new(140.0, 120.0))));
    assert!(!damage
        .iter()
        .any(|rect| rect.contains(Point::new(700.0, 500.0))));
}

#[test]
fn removing_and_reinserting_uses_a_new_generation() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 100.0, 100.0);
    let mut tree = HostTree::new();
    tree.push(UiNode::new(
        UiId::owned("node"),
        UiNodeKind::Group,
        viewport,
    ));
    let first = host.commit(&tree, &interaction, viewport, &mut InvalidationSet::new());
    let first_id = match &first.mutations[0] {
        HostMutation::InsertNode { id, .. } => *id,
        mutation => panic!("unexpected mutation: {mutation:?}"),
    };
    host.commit(
        &HostTree::new(),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let second = host.commit(&tree, &interaction, viewport, &mut InvalidationSet::new());
    let second_id = match &second.mutations[0] {
        HostMutation::InsertNode { id, .. } => *id,
        mutation => panic!("unexpected mutation: {mutation:?}"),
    };
    assert_ne!(first_id, second_id);
}

#[test]
fn removing_a_parent_and_its_child_in_one_commit_keeps_the_old_graph_readable() {
    for iteration in 0..32 {
        let mut host = HostRuntime::new();
        let interaction = UiInteractionState::default();
        let viewport = UiRect::new(0.0, 0.0, 100.0, 100.0);
        let parent = UiId::owned(format!("parent-{iteration}"));
        let child = UiId::owned(format!("child-{iteration}"));
        let mut tree = HostTree::new();
        tree.push(UiNode::new(parent.clone(), UiNodeKind::Clip, viewport).clip(viewport, 0.0, 0.0));
        tree.push(
            UiNode::new(child, UiNodeKind::Panel, viewport)
                .parent(parent)
                .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        host.commit(&tree, &interaction, viewport, &mut InvalidationSet::new());

        let commit = host.commit(
            &HostTree::new(),
            &interaction,
            viewport,
            &mut InvalidationSet::new(),
        );

        assert_eq!(
            commit
                .mutations
                .iter()
                .filter(|mutation| matches!(mutation, HostMutation::RemoveNode { .. }))
                .count(),
            2
        );
    }
}

#[test]
fn paint_change_recompiles_only_its_retained_scene_root() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 200.0, 100.0);
    let first_id = UiId::owned("first");
    let second_id = UiId::owned("second");
    let make_tree = |first_fill| {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                first_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 100.0, 100.0),
            )
            .style(VisualStyle::filled(first_fill)),
        );
        tree.push(
            UiNode::new(
                second_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(100.0, 0.0, 200.0, 100.0),
            )
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        tree
    };
    host.commit(
        &make_tree(crate::core::Color::BLACK),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let commit = host.commit(
        &make_tree(crate::core::Color::WHITE),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );

    assert_eq!(commit.metrics.reused_scene_nodes, 1);
    assert_eq!(
        commit
            .scene_mutations
            .iter()
            .filter(|mutation| matches!(mutation, SceneMutation::Update(_)))
            .count(),
        1
    );
}

#[test]
fn nested_clip_change_recompiles_only_the_clip_scene_root() {
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 240.0, 120.0);
    let clip_id = UiId::owned("clip");
    let child_id = UiId::owned("clip-child");
    let sibling_id = UiId::owned("sibling");
    let make_tree = |fill| {
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                clip_id.clone(),
                UiNodeKind::Clip,
                UiRect::new(0.0, 0.0, 120.0, 120.0),
            )
            .clip(UiRect::new(0.0, 0.0, 120.0, 120.0), 0.0, 0.0),
        );
        tree.push(
            UiNode::new(
                child_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 120.0, 120.0),
            )
            .parent(clip_id.clone())
            .style(VisualStyle::filled(fill)),
        );
        tree.push(
            UiNode::new(
                sibling_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(120.0, 0.0, 240.0, 120.0),
            )
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
        tree
    };
    host.commit(
        &make_tree(crate::core::Color::BLACK),
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
    );
    let next = make_tree(crate::core::Color::WHITE);
    let commit = host.commit(&next, &interaction, viewport, &mut InvalidationSet::new());

    assert_eq!(commit.metrics.reused_scene_nodes, 1);
    assert_eq!(
        commit
            .scene_mutations
            .iter()
            .filter(|mutation| matches!(mutation, SceneMutation::Update(_)))
            .count(),
        1
    );
    assert_eq!(commit.scene.commands(), next.scene().commands());
}

#[test]
fn popup_escapes_ancestor_clip_and_remains_last_in_incremental_scene() {
    let mut tree = HostTree::new();
    let clip_id = UiId::owned("clip");
    tree.push(
        UiNode::new(
            clip_id.clone(),
            UiNodeKind::Clip,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .clip(UiRect::new(0.0, 0.0, 100.0, 100.0), 0.0, 0.0),
    );
    tree.push(
        UiNode::new(
            UiId::owned("clipped-content"),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
        .parent(clip_id.clone())
        .style(VisualStyle::filled(crate::core::Color::BLACK)),
    );
    tree.push(
        UiNode::new(
            UiId::owned("popup"),
            UiNodeKind::Panel,
            UiRect::new(80.0, 80.0, 180.0, 180.0),
        )
        .parent(clip_id)
        .render_phase(crate::core::RenderPhase::Popup)
        .style(VisualStyle::filled(crate::core::Color::WHITE)),
    );
    let mut host = HostRuntime::new();
    let commit = host.commit(
        &tree,
        &UiInteractionState::default(),
        UiRect::new(0.0, 0.0, 200.0, 200.0),
        &mut InvalidationSet::new(),
    );

    assert_eq!(commit.scene.commands(), tree.scene().commands());
    assert_eq!(
        commit.scene.commands().last().map(ScenePrimitive::id),
        Some(&UiId::owned("popup"))
    );
}

#[test]
fn render_phase_change_rebuilds_retained_scene_root_order() {
    let first_id = UiId::owned("first");
    let popup_id = UiId::owned("becomes-popup");
    let last_id = UiId::owned("last");
    let mut tree = HostTree::new();
    for id in [&first_id, &popup_id, &last_id] {
        tree.push(
            UiNode::new(
                id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 20.0, 20.0),
            )
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );
    }
    let mut host = HostRuntime::new();
    let interaction = UiInteractionState::default();
    let viewport = UiRect::new(0.0, 0.0, 20.0, 20.0);
    let changes = tree.take_projection_changes();
    host.commit_projection(
        &tree,
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
        changes,
    );

    let next = tree
        .node(&popup_id)
        .expect("phase-changing node")
        .clone()
        .render_phase(crate::core::RenderPhase::Popup);
    tree.upsert(next);
    let changes = tree.take_projection_changes();
    assert!(!changes.structure_changed);
    let commit = host.commit_projection(
        &tree,
        &interaction,
        viewport,
        &mut InvalidationSet::new(),
        changes,
    );

    assert!(commit.scene_mutations.contains(&SceneMutation::Reorder));
    assert_eq!(
        commit.scene.commands().last().map(ScenePrimitive::id),
        Some(&popup_id)
    );
}
