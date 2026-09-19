use super::*;

pub(super) fn effective_node_paint_bounds(node: &UiNode) -> UiRect {
    let bounds = node
        .compositing_layer
        .map(|spec| spec.transform.transformed_bounds(node.layout_rect))
        .unwrap_or(node.paint_bounds);
    let bounds = node
        .shadow
        .map_or(bounds, |shadow| shadow.paint_bounds(bounds));
    bounds.inflate(node.animation_outset.0, node.animation_outset.1)
}

pub(super) fn compositing_spec_only_changed(previous: &UiNode, next: &UiNode) -> bool {
    if previous.compositing_layer == next.compositing_layer
        || previous.compositing_layer.is_none()
        || next.compositing_layer.is_none()
    {
        return false;
    }
    let mut normalized = previous.clone();
    normalized.compositing_layer = next.compositing_layer;
    normalized.projection_eq(next)
}

pub(super) fn child_structure_damage(
    host: &HostRuntime,
    tree: &HostTree,
    previous: &[HostNodeId],
    next: &[HostNodeId],
    fallback: UiRect,
) -> UiRect {
    let previous_positions = previous
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<HashMap<_, _>>();
    let next_positions = next
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<HashMap<_, _>>();
    let mut affected = previous
        .iter()
        .chain(next)
        .copied()
        .filter(|id| previous_positions.get(id) != next_positions.get(id))
        .collect::<HashSet<_>>();
    let mut bounds = None;
    for id in affected.drain() {
        let Some(node) = host.try_node(id) else {
            continue;
        };
        if node.mounted {
            bounds = Some(bounds.map_or(node.paint_bounds, |bounds: UiRect| {
                bounds.union(node.paint_bounds)
            }));
        }
        if let Some(next) = tree.node(&node.source) {
            bounds = Some(bounds.map_or(next.paint_bounds, |bounds: UiRect| {
                bounds.union(next.paint_bounds)
            }));
        }
    }
    bounds.unwrap_or(fallback)
}

pub(super) fn paint_props_changed(previous: &UiNode, next: &UiNode) -> bool {
    previous.kind != next.kind
        || previous.layout_rect != next.layout_rect
        || previous.paint_bounds != next.paint_bounds
        || previous.style != next.style
        || previous.path != next.path
        || previous.path_style != next.path_style
        || previous.image_request != next.image_request
        || previous.image_fit != next.image_fit
        || previous.image_blur != next.image_blur
        || previous.icon_key != next.icon_key
        || previous.icon_style != next.icon_style
        || previous.glow != next.glow
        || previous.backdrop_blur_style != next.backdrop_blur_style
        || previous.content_blur_style != next.content_blur_style
        || previous.overlay_style != next.overlay_style
        || previous.custom_style != next.custom_style
        || previous.compositing_layer != next.compositing_layer
        || previous.shadow != next.shadow
        || previous.static_layer != next.static_layer
        || previous.scroll_raster != next.scroll_raster
        || previous.clip_rect != next.clip_rect
        || previous.content_offset != next.content_offset
        || previous.text != next.text
        || previous.text_style != next.text_style
        || previous.render_phase != next.render_phase
}
