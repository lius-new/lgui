use super::*;

impl HostTree {
    pub(crate) fn has_shadow_ancestor(&self, id: &UiId) -> bool {
        self.shadow_root(id).is_some()
    }

    fn shadow_root(&self, id: &UiId) -> Option<&UiNode> {
        let mut current = self.node(id);
        let mut root = None;
        while let Some(node) = current {
            if node.shadow.is_some() {
                root = Some(node);
            }
            current = node.parent.as_ref().and_then(|parent| self.node(parent));
            if node.render_phase == super::super::RenderPhase::Popup
                && current.is_some_and(|parent| parent.render_phase != node.render_phase)
            {
                break;
            }
        }
        root
    }

    pub fn paint_bounds(&self, ids: impl IntoIterator<Item = UiId>) -> Option<UiRect> {
        ids.into_iter()
            .filter_map(|id| {
                let node = self.node(&id)?;
                let mut bounds = node_dirty_bounds(node);
                if let Some(root) = self.shadow_root(&id) {
                    for command in crate::core::compile_scene_root(self, &root.id).commands() {
                        bounds = bounds.union(command.paint_bounds());
                    }
                }
                Some(bounds)
            })
            .reduce(UiRect::union)
    }

    pub fn full_paint_bounds(&self) -> Option<UiRect> {
        let bounds = self
            .nodes
            .iter()
            .map(|node| node.paint_bounds)
            .reduce(UiRect::union);
        self.nodes
            .iter()
            .filter(|node| node.shadow.is_some())
            .fold(bounds, |bounds, node| {
                crate::core::compile_scene_root(self, &node.id)
                    .commands()
                    .iter()
                    .fold(bounds, |bounds, command| {
                        Some(bounds.map_or(command.paint_bounds(), |bounds| {
                            bounds.union(command.paint_bounds())
                        }))
                    })
            })
    }

    pub fn scene(&self) -> Scene {
        compile_scene(self)
    }
}

fn node_dirty_bounds(node: &UiNode) -> UiRect {
    let (x, y) = node.animation_outset;
    node.paint_bounds.inflate(x, y)
}
