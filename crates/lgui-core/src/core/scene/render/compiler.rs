use super::{primitive::*, scene::*, scroll_cache::*, signature::*, transform::*, *};

pub fn compile_scene(tree: &HostTree) -> Scene {
    let mut list = Scene::new();
    let mut skip = HashSet::new();
    for node in tree.nodes() {
        if skip.contains(&node.id) {
            continue;
        }
        let command_start = list.commands.len();
        let include_popup_subtree = node.render_phase == RenderPhase::Popup;
        if node.shadow.is_some() {
            push_node_and_children(
                tree,
                node,
                Arc::make_mut(&mut list.commands),
                &mut skip,
                include_popup_subtree,
            );
            translate_popup_root_commands(&mut list, tree, node, command_start);
            continue;
        }
        if matches!(
            node.kind,
            UiNodeKind::CompositingLayer
                | UiNodeKind::StaticLayer
                | UiNodeKind::ScrollRaster
                | UiNodeKind::Clip
                | UiNodeKind::ClipPath
        ) {
            if let Some(spec) = node.compositing_layer {
                let (commands, content_signature) = compile_compositing_layer_commands(
                    tree,
                    node,
                    &mut skip,
                    include_popup_subtree,
                );
                list.push(ScenePrimitive::CompositingLayer {
                    id: node.id.clone(),
                    rect: node.layout_rect,
                    spec,
                    commands,
                    content_signature,
                    phase: node.render_phase,
                });
            }
            if let Some(spec) = node.static_layer.clone() {
                let commands =
                    compile_static_layer_commands(tree, node, &mut skip, include_popup_subtree);
                let child_signature = command_signature(&commands);
                list.push(ScenePrimitive::StaticLayer {
                    id: node.id.clone(),
                    rect: node.layout_rect,
                    spec,
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
            if let Some(spec) = node.scroll_raster.clone() {
                let (commands, child_signature) = compile_scroll_raster_commands(
                    tree,
                    node,
                    &spec,
                    &mut skip,
                    include_popup_subtree,
                );
                list.push(ScenePrimitive::ScrollRaster {
                    id: node.id.clone(),
                    viewport: node.layout_rect,
                    spec,
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
            if node.kind == UiNodeKind::Clip {
                let commands = compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
                let child_signature = command_signature(&commands);
                list.push(ScenePrimitive::Clip {
                    id: node.id.clone(),
                    rect: node.clip_rect.unwrap_or(node.layout_rect),
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
            if node.kind == UiNodeKind::ClipPath {
                if let Some(path) = node.path.clone() {
                    let commands =
                        compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
                    let child_signature = command_signature(&commands);
                    list.push(ScenePrimitive::ClipPath {
                        id: node.id.clone(),
                        rect: node.clip_rect.unwrap_or(node.layout_rect),
                        path,
                        commands,
                        child_signature,
                        phase: node.render_phase,
                    });
                }
            }
            translate_popup_root_commands(&mut list, tree, node, command_start);
            continue;
        }
        if matches!(node.kind, UiNodeKind::Group | UiNodeKind::Root) {
            continue;
        }
        push_node_commands(&mut list, node);
        translate_popup_root_commands(&mut list, tree, node, command_start);
    }
    list.move_popup_commands_to_end();
    list
}

/// Returns the retained scene roots in backend paint order without compiling their primitives.
/// Descendants owned by clip/static/raster containers are represented by that container root;
/// popup descendants remain independent roots so they preserve popup z-order semantics.
pub fn scene_root_ids(tree: &HostTree) -> Vec<UiId> {
    let mut roots = Vec::new();
    let mut skip = HashSet::new();
    for node in tree.nodes() {
        if skip.contains(&node.id) {
            continue;
        }
        if node.shadow.is_some()
            || matches!(
                node.kind,
                UiNodeKind::CompositingLayer
                    | UiNodeKind::StaticLayer
                    | UiNodeKind::ScrollRaster
                    | UiNodeKind::Clip
                    | UiNodeKind::ClipPath
            )
        {
            roots.push(node.id.clone());
            mark_node_and_descendants_skipped(
                tree,
                node,
                &mut skip,
                node.render_phase == RenderPhase::Popup,
            );
        } else if !matches!(node.kind, UiNodeKind::Group | UiNodeKind::Root) {
            roots.push(node.id.clone());
        }
    }
    roots.sort_by_key(|id| {
        tree.node(id)
            .is_some_and(|node| node.render_phase == RenderPhase::Popup)
    });
    roots
}

/// Compiles one retained scene root. Normal commits call this only for inserted or paint-dirty
/// roots; unchanged roots keep their previously committed primitive vectors.
pub fn compile_scene_root(tree: &HostTree, id: &UiId) -> Scene {
    let Some(node) = tree.node(id) else {
        return Scene::new();
    };
    let mut list = Scene::new();
    let mut skip = HashSet::new();
    let include_popup_subtree = node.render_phase == RenderPhase::Popup;
    if node.shadow.is_some() {
        push_node_and_children(
            tree,
            node,
            Arc::make_mut(&mut list.commands),
            &mut skip,
            include_popup_subtree,
        );
        translate_popup_root_commands(&mut list, tree, node, 0);
        return list;
    }
    if matches!(
        node.kind,
        UiNodeKind::CompositingLayer
            | UiNodeKind::StaticLayer
            | UiNodeKind::ScrollRaster
            | UiNodeKind::Clip
            | UiNodeKind::ClipPath
    ) {
        if let Some(spec) = node.compositing_layer {
            let (commands, content_signature) =
                compile_compositing_layer_commands(tree, node, &mut skip, include_popup_subtree);
            list.push(ScenePrimitive::CompositingLayer {
                id: node.id.clone(),
                rect: node.layout_rect,
                spec,
                commands,
                content_signature,
                phase: node.render_phase,
            });
        }
        if let Some(spec) = node.static_layer.clone() {
            let commands =
                compile_static_layer_commands(tree, node, &mut skip, include_popup_subtree);
            let child_signature = command_signature(&commands);
            list.push(ScenePrimitive::StaticLayer {
                id: node.id.clone(),
                rect: node.layout_rect,
                spec,
                commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        if let Some(spec) = node.scroll_raster.clone() {
            let (commands, child_signature) =
                compile_scroll_raster_commands(tree, node, &spec, &mut skip, include_popup_subtree);
            list.push(ScenePrimitive::ScrollRaster {
                id: node.id.clone(),
                viewport: node.layout_rect,
                spec,
                commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        if node.kind == UiNodeKind::Clip {
            let commands = compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
            let child_signature = command_signature(&commands);
            list.push(ScenePrimitive::Clip {
                id: node.id.clone(),
                rect: node.clip_rect.unwrap_or(node.layout_rect),
                commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        if node.kind == UiNodeKind::ClipPath {
            if let Some(path) = node.path.clone() {
                let commands = compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
                let child_signature = command_signature(&commands);
                list.push(ScenePrimitive::ClipPath {
                    id: node.id.clone(),
                    rect: node.clip_rect.unwrap_or(node.layout_rect),
                    path,
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
        }
    } else if !matches!(node.kind, UiNodeKind::Group | UiNodeKind::Root) {
        push_node_commands(&mut list, node);
    }
    translate_popup_root_commands(&mut list, tree, node, 0);
    list
}

fn translate_popup_root_commands(
    list: &mut Scene,
    tree: &HostTree,
    node: &UiNode,
    command_start: usize,
) {
    if node.render_phase != RenderPhase::Popup {
        return;
    }
    let (offset_x, offset_y) = tree.ancestor_content_offset(node);
    if offset_x == 0.0 && offset_y == 0.0 {
        return;
    }
    for command in &mut Arc::make_mut(&mut list.commands)[command_start..] {
        *command = translate_command(command, offset_x, offset_y);
    }
}

fn compile_static_layer_commands(
    tree: &HostTree,
    root: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> Vec<ScenePrimitive> {
    let mut commands = Vec::new();
    for child_id in &root.children {
        compile_static_layer_child(tree, child_id, &mut commands, skip, include_popup_subtree);
    }
    commands
}

fn compile_compositing_layer_commands(
    tree: &HostTree,
    root: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> (Vec<ScenePrimitive>, u64) {
    let commands = compile_static_layer_commands(tree, root, skip, include_popup_subtree);
    let commands = translate_commands(commands, -root.layout_rect.left, -root.layout_rect.top);
    let content_signature = command_signature(&commands);
    (commands, content_signature)
}

fn compile_scroll_raster_commands(
    tree: &HostTree,
    root: &UiNode,
    spec: &ScrollRasterSpec,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> (Vec<ScenePrimitive>, u64) {
    if let Some(snapshot) = load_scroll_raster_command_snapshot(root, spec) {
        mark_node_and_descendants_skipped(tree, root, skip, include_popup_subtree);
        return snapshot;
    }
    let commands = compile_static_layer_commands(tree, root, skip, include_popup_subtree);
    let child_signature = command_signature(&commands);
    store_scroll_raster_command_snapshot(root, spec, commands.clone(), child_signature);
    (commands, child_signature)
}

fn mark_node_and_descendants_skipped(
    tree: &HostTree,
    node: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) {
    skip.insert(node.id.clone());
    for child_id in &node.children {
        if let Some(child) = tree.node(child_id) {
            if child.render_phase == RenderPhase::Popup && !include_popup_subtree {
                continue;
            }
            mark_node_and_descendants_skipped(tree, child, skip, include_popup_subtree);
        }
    }
}

fn compile_static_layer_child(
    tree: &HostTree,
    id: &UiId,
    commands: &mut Vec<ScenePrimitive>,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) {
    let Some(node) = tree.node(id) else {
        return;
    };
    if node.render_phase == RenderPhase::Popup && !include_popup_subtree {
        return;
    }
    skip.insert(node.id.clone());
    push_node_and_children(tree, node, commands, skip, include_popup_subtree);
}

fn compile_clip_commands(
    tree: &HostTree,
    root: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> Vec<ScenePrimitive> {
    let mut commands = Vec::new();
    let (offset_x, offset_y) = root.content_offset;
    for child_id in &root.children {
        compile_clip_child(
            tree,
            child_id,
            &mut commands,
            skip,
            offset_x,
            offset_y,
            include_popup_subtree,
        );
    }
    commands
}

fn compile_clip_child(
    tree: &HostTree,
    id: &UiId,
    commands: &mut Vec<ScenePrimitive>,
    skip: &mut HashSet<UiId>,
    offset_x: f32,
    offset_y: f32,
    include_popup_subtree: bool,
) {
    let Some(node) = tree.node(id) else {
        return;
    };
    if node.render_phase == RenderPhase::Popup && !include_popup_subtree {
        return;
    }
    let before = commands.len();
    push_node_and_children(tree, node, commands, skip, include_popup_subtree);
    for command in &mut commands[before..] {
        *command = translate_command(command, offset_x, offset_y);
    }
}

fn push_node_and_children(
    tree: &HostTree,
    node: &UiNode,
    commands: &mut Vec<ScenePrimitive>,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) {
    if node.render_phase == RenderPhase::Popup && !include_popup_subtree {
        return;
    }
    skip.insert(node.id.clone());
    if let Some(shadow) = node.shadow {
        let mut source = node.clone();
        source.shadow = None;
        let mut nested = Vec::new();
        push_node_and_children(tree, &source, &mut nested, skip, include_popup_subtree);
        let Some(content_bounds) = nested
            .iter()
            .map(ScenePrimitive::paint_bounds)
            .reduce(UiRect::union)
        else {
            return;
        };
        // A shadowed compositing node needs two independent retained surfaces.
        for command in &mut nested {
            if let ScenePrimitive::CompositingLayer { id, .. } = command {
                if *id == node.id {
                    *id = UiId::owned(format!("{}.__shadow_content", node.id.as_str()));
                }
            }
        }
        let rect = shadow.paint_bounds(content_bounds.union(node.paint_bounds));
        let rect = UiRect::new(
            rect.left.floor(),
            rect.top.floor(),
            rect.right.ceil(),
            rect.bottom.ceil(),
        );
        let nested = translate_commands(nested, -rect.left, -rect.top);
        let content_signature = command_signature(&nested);
        let mut spec = CompositingLayerSpec::new();
        spec.shadow = Some(shadow);
        commands.push(ScenePrimitive::CompositingLayer {
            id: node.id.clone(),
            rect,
            spec,
            commands: nested,
            content_signature,
            phase: node.render_phase,
        });
        return;
    }
    if node.kind == UiNodeKind::Clip {
        let nested = compile_clip_commands(tree, node, skip, include_popup_subtree);
        let child_signature = command_signature(&nested);
        commands.push(ScenePrimitive::Clip {
            id: node.id.clone(),
            rect: node.clip_rect.unwrap_or(node.layout_rect),
            commands: nested,
            child_signature,
            phase: node.render_phase,
        });
        return;
    }
    if node.kind == UiNodeKind::ClipPath {
        if let Some(path) = node.path.clone() {
            let nested = compile_clip_commands(tree, node, skip, include_popup_subtree);
            let child_signature = command_signature(&nested);
            commands.push(ScenePrimitive::ClipPath {
                id: node.id.clone(),
                rect: node.clip_rect.unwrap_or(node.layout_rect),
                path,
                commands: nested,
                child_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if let UiNodeKind::CompositingLayer = node.kind {
        if let Some(spec) = node.compositing_layer {
            let (layer_commands, content_signature) =
                compile_compositing_layer_commands(tree, node, skip, include_popup_subtree);
            commands.push(ScenePrimitive::CompositingLayer {
                id: node.id.clone(),
                rect: node.layout_rect,
                spec,
                commands: layer_commands,
                content_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if let UiNodeKind::StaticLayer = node.kind {
        if let Some(spec) = node.static_layer.clone() {
            let layer_commands =
                compile_static_layer_commands(tree, node, skip, include_popup_subtree);
            let child_signature = command_signature(&layer_commands);
            commands.push(ScenePrimitive::StaticLayer {
                id: node.id.clone(),
                rect: node.layout_rect,
                spec,
                commands: layer_commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if let UiNodeKind::ScrollRaster = node.kind {
        if let Some(spec) = node.scroll_raster.clone() {
            let (raster_commands, child_signature) =
                compile_scroll_raster_commands(tree, node, &spec, skip, include_popup_subtree);
            commands.push(ScenePrimitive::ScrollRaster {
                id: node.id.clone(),
                viewport: node.layout_rect,
                spec,
                commands: raster_commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if !matches!(
        node.kind,
        UiNodeKind::Group
            | UiNodeKind::Root
            | UiNodeKind::CompositingLayer
            | UiNodeKind::StaticLayer
            | UiNodeKind::ScrollRaster
            | UiNodeKind::ClipPath
    ) {
        push_node_commands_vec(commands, node);
    }
    for child_id in &node.children {
        let Some(child) = tree.node(child_id) else {
            continue;
        };
        if child.render_phase == RenderPhase::Popup && !include_popup_subtree {
            continue;
        }
        push_node_and_children(tree, child, commands, skip, include_popup_subtree);
    }
}

fn push_node_commands(list: &mut Scene, node: &UiNode) {
    push_node_commands_into(|command| list.push(command), node);
}

fn push_node_commands_vec(commands: &mut Vec<ScenePrimitive>, node: &UiNode) {
    push_node_commands_into(|command| commands.push(command), node);
}

fn push_node_commands_into(mut push: impl FnMut(ScenePrimitive), node: &UiNode) {
    if !matches!(node.kind, UiNodeKind::Ellipse)
        && (node.style.fill.is_some() || node.style.stroke.is_some())
    {
        push(ScenePrimitive::Rect {
            id: node.id.clone(),
            rect: node.layout_rect,
            style: node.style,
            phase: node.render_phase,
        });
    }
    if let UiNodeKind::Ellipse = node.kind {
        if node.style.fill.is_some() || node.style.stroke.is_some() {
            push(ScenePrimitive::Ellipse {
                id: node.id.clone(),
                rect: node.layout_rect,
                style: node.style,
                phase: node.render_phase,
            });
        }
    }
    if let (Some(text), Some(style)) = (node.text.clone(), node.text_style) {
        push(ScenePrimitive::Text {
            id: node.id.clone(),
            rect: node.layout_rect,
            text,
            style,
            phase: node.render_phase,
        });
    }
    if let UiNodeKind::Custom(key) = node.kind {
        push(ScenePrimitive::Custom {
            id: node.id.clone(),
            rect: node.layout_rect,
            key,
            style: node.custom_style,
            phase: node.render_phase,
        });
    }
    if let UiNodeKind::Line = node.kind {
        if let Some(stroke) = node.style.stroke {
            push(ScenePrimitive::Line {
                id: node.id.clone(),
                start: super::Point::new(node.layout_rect.left, node.layout_rect.top),
                end: super::Point::new(node.layout_rect.right, node.layout_rect.bottom),
                stroke,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Path = node.kind {
        if let Some(path) = node.path.clone() {
            push(ScenePrimitive::Path {
                id: node.id.clone(),
                rect: node.layout_rect,
                path,
                style: node.path_style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Image = node.kind {
        if let Some(request) = node.image_request.as_ref() {
            push(ScenePrimitive::Image {
                id: node.id.clone(),
                rect: node.layout_rect,
                source: request.source().clone(),
                request: request.clone(),
                fit: node.image_fit,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Icon = node.kind {
        if let Some(key) = node.icon_key {
            push(ScenePrimitive::Icon {
                id: node.id.clone(),
                rect: node.layout_rect,
                key,
                style: node.icon_style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Glow = node.kind {
        if let Some((color, alpha)) = node.glow {
            push(ScenePrimitive::Glow {
                id: node.id.clone(),
                rect: node.layout_rect,
                color,
                alpha,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::BackdropBlur = node.kind {
        if let Some(style) = node.backdrop_blur_style {
            push(ScenePrimitive::BackdropBlur {
                id: node.id.clone(),
                rect: node.layout_rect,
                style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::BackdropBlurPath = node.kind {
        if let (Some(style), Some(path)) = (node.backdrop_blur_style, node.path.clone()) {
            push(ScenePrimitive::BackdropBlurPath {
                id: node.id.clone(),
                rect: node.layout_rect,
                path,
                style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Overlay = node.kind {
        if let Some(style) = node.overlay_style.clone() {
            push(ScenePrimitive::Overlay {
                id: node.id.clone(),
                rect: node.layout_rect,
                style,
                phase: node.render_phase,
            });
        }
    }
}
