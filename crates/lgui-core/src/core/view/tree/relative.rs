use super::*;
use crate::core::RelativeRelation;

impl HostTree {
    /// Resolve after native projection, before layout. Missing targets remain
    /// under their original staging parent; identities and callbacks survive.
    pub fn resolve_relative_placements(&mut self) -> bool {
        let mut placed: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|node| {
                node.placement.clone().map(|placement| {
                    (
                        node.id.clone(),
                        node.public_id.clone(),
                        node.placement_source.clone(),
                        placement,
                    )
                })
            })
            .collect();
        if placed.is_empty() {
            return false;
        }
        let public_ids: HashMap<_, _> = self
            .nodes
            .iter()
            .filter_map(|node| {
                node.public_id
                    .clone()
                    .map(|public| (public, node.id.clone()))
            })
            .collect();
        placed.sort_by(|a, b| {
            (a.3.order, a.1.as_deref().unwrap_or(a.0.as_str()))
                .cmp(&(b.3.order, b.1.as_deref().unwrap_or(b.0.as_str())))
        });
        let ids: HashSet<_> = placed.iter().map(|p| p.0.clone()).collect();
        let mut children: HashMap<UiId, Vec<UiId>> = HashMap::new();
        for (id, _, source, _) in &placed {
            for parent in [
                self.node(id).and_then(|node| node.parent.as_ref()),
                source.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                children.entry(parent.clone()).or_insert_with(|| {
                    self.node(parent).map_or_else(Vec::new, |node| {
                        node.children
                            .iter()
                            .filter(|child| !ids.contains(*child))
                            .cloned()
                            .collect()
                    })
                });
            }
        }
        let mut parents: HashMap<UiId, Option<UiId>> = HashMap::new();
        for (id, _, source, _) in &placed {
            parents.insert(id.clone(), source.clone());
        }
        let mut resolved = HashSet::new();
        let mut pending: Vec<_> = placed.iter().collect();
        while !pending.is_empty() {
            let mut progress = false;
            pending.retain(|(id, _, source, placement)| {
                let target = public_ids
                    .get(placement.target.as_ref())
                    .and_then(|id| self.node(id));
                if target.is_some_and(|target| {
                    ids.contains(&target.id) && !resolved.contains(&target.id)
                }) {
                    return true;
                }
                let destination = target.and_then(|target| {
                    if target.visibility == crate::core::Visibility::Collapsed {
                        return None;
                    }
                    if placement.relation == RelativeRelation::Append
                        && !matches!(
                            target.kind,
                            crate::core::UiNodeKind::Root
                                | crate::core::UiNodeKind::Group
                                | crate::core::UiNodeKind::Panel
                                | crate::core::UiNodeKind::Button
                        )
                    {
                        return None;
                    }
                    let parent = if placement.relation == RelativeRelation::Append {
                        Some(target.id.clone())
                    } else {
                        parents
                            .get(&target.id)
                            .cloned()
                            .unwrap_or_else(|| target.parent.clone())
                    }?;
                    let mut ancestor = Some(parent.clone());
                    let mut seen = HashSet::new();
                    while let Some(current) = ancestor {
                        if current == *id {
                            return None;
                        }
                        if !seen.insert(current.clone()) {
                            return None;
                        }
                        ancestor = parents.get(&current).cloned().unwrap_or_else(|| {
                            self.node(&current).and_then(|node| node.parent.clone())
                        });
                    }
                    Some((parent, target.id.clone()))
                });
                let (parent, target) = destination
                    .map(|(p, t)| (Some(p), Some(t)))
                    .unwrap_or((source.clone(), None));
                if let Some(parent) = &parent {
                    let siblings = children.entry(parent.clone()).or_insert_with(|| {
                        self.node(parent).map_or_else(Vec::new, |node| {
                            node.children
                                .iter()
                                .filter(|child| !ids.contains(*child))
                                .cloned()
                                .collect()
                        })
                    });
                    let index = match (&target, placement.relation) {
                        (Some(target), RelativeRelation::Before) => siblings
                            .iter()
                            .position(|s| s == target)
                            .unwrap_or(siblings.len()),
                        (Some(target), RelativeRelation::After) => {
                            let at = siblings
                                .iter()
                                .position(|s| s == target)
                                .map_or(siblings.len(), |at| at + 1);
                            at + siblings[at..]
                                .iter()
                                .take_while(|s| ids.contains(*s))
                                .count()
                        }
                        _ => siblings.len(),
                    };
                    siblings.insert(index, id.clone());
                }
                parents.insert(id.clone(), parent);
                resolved.insert(id.clone());
                progress = true;
                false
            });
            if !progress {
                // Cycles and targets depending on a cycle stay hidden in staging.
                for (id, _, source, _) in pending.drain(..) {
                    if let Some(parent) = source {
                        children.entry(parent.clone()).or_default().push(id.clone());
                    }
                }
            }
        }
        let mut changed = false;
        for (id, _, _, _) in placed {
            let parent = parents.remove(&id).flatten();
            if self.node(&id).is_some_and(|node| node.parent != parent) {
                self.node_mut(&id).unwrap().parent = parent;
                changed = true;
            }
        }
        for (parent, next) in children {
            if self.node(&parent).is_some_and(|node| node.children != next) {
                self.set_children(&parent, next);
                changed = true;
            }
        }
        if changed {
            self.projection_changes.structure_changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{RelativePlacement, UiNodeKind, Visibility};
    fn id(value: &str) -> UiId {
        UiId::owned(value.to_owned())
    }
    fn fixture() -> HostTree {
        let mut tree = HostTree::new();
        tree.push(UiNode::new(
            id("root"),
            UiNodeKind::Group,
            UiRect::default(),
        ));
        tree.push(
            UiNode::new(id("target"), UiNodeKind::Group, UiRect::default())
                .parent(id("root"))
                .public_id("arbitrary.native"),
        );
        tree.push(
            UiNode::new(id("stage"), UiNodeKind::Group, UiRect::default())
                .parent(id("root"))
                .visibility(Visibility::Collapsed),
        );
        tree
    }
    fn insert(
        tree: &mut HostTree,
        name: &str,
        target: &str,
        relation: RelativeRelation,
        order: i32,
    ) {
        let mut node = UiNode::new(id(name), UiNodeKind::Group, UiRect::default())
            .parent(id("stage"))
            .public_id(name);
        node.placement_source = Some(id("stage"));
        node.placement = Some(RelativePlacement {
            target: target.into(),
            relation,
            order,
        });
        tree.push(node);
    }
    #[test]
    fn relative_order_is_stable_and_repeated_projection_is_idempotent() {
        let mut tree = fixture();
        insert(
            &mut tree,
            "b",
            "arbitrary.native",
            RelativeRelation::Before,
            5,
        );
        insert(
            &mut tree,
            "a",
            "arbitrary.native",
            RelativeRelation::Before,
            5,
        );
        insert(
            &mut tree,
            "c",
            "arbitrary.native",
            RelativeRelation::After,
            -10,
        );
        tree.resolve_relative_placements();
        assert_eq!(
            tree.node(&id("root")).unwrap().children,
            vec![id("a"), id("b"), id("target"), id("c"), id("stage")]
        );
        assert!(!tree.resolve_relative_placements());
    }
    #[test]
    fn missing_targets_rebind_and_owned_relative_targets_resolve_in_dependency_order() {
        let mut tree = fixture();
        insert(&mut tree, "child", "parent", RelativeRelation::Before, -1);
        insert(
            &mut tree,
            "parent",
            "future.component",
            RelativeRelation::Before,
            1,
        );
        tree.resolve_relative_placements();
        assert_eq!(tree.node(&id("parent")).unwrap().parent, Some(id("stage")));
        tree.push(
            UiNode::new(id("future"), UiNodeKind::Group, UiRect::default())
                .parent(id("root"))
                .public_id("future.component"),
        );
        tree.resolve_relative_placements();
        let siblings = &tree.node(&id("root")).unwrap().children;
        let at = siblings
            .iter()
            .position(|node| *node == id("future"))
            .unwrap();
        assert_eq!(&siblings[at - 2..at], &[id("child"), id("parent")]);
        tree.remove_ids(&HashSet::from([id("future")]));
        tree.resolve_relative_placements();
        assert_eq!(tree.node(&id("child")).unwrap().parent, Some(id("stage")));
    }
    #[test]
    fn relative_cycles_return_to_staging_without_changing_identity() {
        let mut tree = fixture();
        insert(&mut tree, "one", "two", RelativeRelation::Append, 0);
        insert(&mut tree, "two", "one", RelativeRelation::Append, 0);
        tree.resolve_relative_placements();
        assert_eq!(tree.node(&id("one")).unwrap().parent, Some(id("stage")));
        assert_eq!(tree.node(&id("two")).unwrap().parent, Some(id("stage")));
        assert_eq!(tree.node(&id("stage")).unwrap().children.len(), 2);
    }
}
