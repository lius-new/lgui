use super::*;

impl HostTree {
    pub fn paint_bounds(&self, ids: impl IntoIterator<Item = UiId>) -> Option<UiRect> {
        ids.into_iter()
            .filter_map(|id| self.node(&id).map(|node| node_dirty_bounds(node)))
            .reduce(UiRect::union)
    }

    pub fn full_paint_bounds(&self) -> Option<UiRect> {
        self.nodes
            .iter()
            .map(|node| node.paint_bounds)
            .reduce(UiRect::union)
    }

    pub fn scene(&self) -> Scene {
        compile_scene(self)
    }
}

fn node_dirty_bounds(node: &UiNode) -> UiRect {
    let (x, y) = node.animation_outset;
    node.paint_bounds.inflate(x, y)
}
