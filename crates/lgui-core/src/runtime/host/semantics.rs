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
                .into_iter()
                .map(|node| node.as_ref())
                .collect()
        };
        let mut nodes = Vec::new();
        for node in candidates {
            let mut semantic = SemanticNode::from_ui_node(node);
            semantic.semantics.state.hidden |= !tree.is_visible(&node.id);
            semantic.semantics.state.disabled |=
                tree.is_visible(&node.id) && !tree.is_interactive(&node.id);
            // Only the change test needs the previous value, so a fingerprint is kept instead of
            // a second copy of every node's semantics.
            let fingerprint = fingerprint(&semantic);
            if full || self.semantics.get(&node.id) != Some(&fingerprint) {
                nodes.push(semantic);
            }
            self.semantics.insert(node.id.clone(), fingerprint);
        }
        SemanticUpdate {
            nodes,
            removed: removed.into_iter().collect(),
            focus,
            full,
        }
    }
}

/// A 64-bit fingerprint of a semantic node: equal nodes have equal fingerprints, and a different
/// node differs unless two 64-bit hashes collide.
fn fingerprint(node: &SemanticNode) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{node:?}").hash(&mut hasher);
    hasher.finish()
}
