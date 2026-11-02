use super::*;

impl HostRuntime {
    pub(super) fn reconcile_semantics(
        &mut self,
        tree: &HostTree,
        changed: HashSet<UiId>,
        removed: HashSet<UiId>,
        focus: Option<UiId>,
        full: bool,
    ) -> SemanticUpdate {
        for id in &removed {
            self.semantics.remove(id);
        }
        let candidates: Vec<&UiNode> = if full {
            tree.nodes().iter().map(|node| node.as_ref()).collect()
        } else {
            tree.changed_nodes(&changed)
        };
        let mut nodes = Vec::new();
        for node in candidates {
            let mut semantic = SemanticNode::from_ui_node(node);
            semantic.semantics.state.hidden |= !tree.is_visible(&node.id);
            semantic.semantics.state.disabled |=
                tree.is_visible(&node.id) && !tree.is_interactive(&node.id);
            if full || self.semantics.get(&node.id) != Some(&semantic) {
                nodes.push(semantic.clone());
            }
            self.semantics.insert(node.id.clone(), semantic);
        }
        SemanticUpdate {
            nodes,
            removed: removed.into_iter().collect(),
            focus,
            full,
        }
    }
}
