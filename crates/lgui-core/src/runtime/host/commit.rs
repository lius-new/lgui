use super::{reconcile::*, *};

impl HostRuntime {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn commit(
        &mut self,
        tree: &HostTree,
        interaction: &UiInteractionState,
        viewport: UiRect,
        invalidations: &mut InvalidationSet,
    ) -> HostCommit {
        let next_sources = tree
            .nodes()
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let changes = crate::core::ProjectionChanges {
            changed: next_sources.clone(),
            removed: self
                .sources
                .keys()
                .filter(|source| !next_sources.contains(*source))
                .cloned()
                .collect(),
            structure_changed: true,
            ..crate::core::ProjectionChanges::default()
        };
        self.commit_projection(tree, interaction, viewport, invalidations, changes)
    }

    pub(crate) fn commit_projection(
        &mut self,
        tree: &HostTree,
        interaction: &UiInteractionState,
        viewport: UiRect,
        invalidations: &mut InvalidationSet,
        changes: crate::core::ProjectionChanges,
    ) -> HostCommit {
        #[cfg(feature = "diagnostics-timing")]
        let change_scan_started = Instant::now();
        let semantic_changed = changes.changed.clone();
        let semantic_removed = changes.removed.clone();
        let semantic_full = !self.initialized;
        let mut mutations = Vec::new();
        let mut dirty_scene_sources = HashSet::new();
        let mut compositing_updates = HashMap::new();

        let kind_changed_sources = changes
            .changed
            .iter()
            .filter_map(|source| {
                let id = self.sources.get(source)?;
                tree.node(source)
                    .is_some_and(|next| self.node(*id).kind != next.kind)
                    .then(|| source.clone())
            })
            .collect::<Vec<_>>();
        let scene_topology_changed = changes.changed.iter().any(|source| {
            let Some(id) = self.sources.get(source) else {
                return false;
            };
            let Some(next) = tree.node(source) else {
                return false;
            };
            let previous = &self.node(*id).node;
            previous.parent != next.parent
                || previous.kind != next.kind
                || previous.render_phase != next.render_phase
                || previous.shadow.is_some() != next.shadow.is_some()
        });
        let scene_structure_changed =
            changes.structure_changed || !changes.removed.is_empty() || scene_topology_changed;
        #[cfg(feature = "diagnostics-timing")]
        let change_scan_ms = elapsed_ms(change_scan_started);
        #[cfg(feature = "diagnostics-timing")]
        let node_patch_started = Instant::now();
        let mut removed_sources = changes.removed.clone();
        removed_sources.extend(kind_changed_sources);
        let removed = removed_sources
            .into_iter()
            .filter_map(|source| self.sources.get(&source).copied().map(|id| (source, id)))
            .collect::<Vec<_>>();
        // Snapshot every removal while the old parent graph is still intact.
        // A projection may remove a parent and its descendants in the same
        // commit, so releasing either one before this pass would leave stale
        // HostNodeIds in the remaining nodes' ancestry.
        for (source, id) in &removed {
            let old_bounds = self.node(*id).paint_bounds;
            if let Some(owner) = self.scene_owner_source(*id) {
                if let Some(bounds) = self
                    .sources
                    .get(&owner)
                    .and_then(|id| self.scene.get(id))
                    .and_then(|scene| super::scene::shadow_bounds(&scene.commands))
                {
                    invalidations.invalidate_rect(bounds);
                }
                dirty_scene_sources.insert(owner);
            }
            mutations.push(HostMutation::RemoveNode {
                id: *id,
                source: source.clone(),
                old_bounds,
            });
        }
        for (_, id) in removed {
            self.remove(id);
        }

        let initializing = !self.initialized;
        let changed_nodes = if initializing {
            tree.nodes().iter().map(|node| node.as_ref()).collect()
        } else {
            tree.changed_nodes(&changes.changed)
        };
        for node in &changed_nodes {
            if !self.sources.contains_key(&node.id) {
                let id = self.allocate(node.id.clone());
                self.sources.insert(node.id.clone(), id);
            }
        }

        let order_changed = if changes.structure_changed || !self.initialized {
            let next_order = tree
                .nodes()
                .iter()
                .map(|node| self.sources[&node.id])
                .collect::<Vec<_>>();
            let changed = self.initialized && self.paint_order != next_order;
            self.paint_order = next_order;
            changed
        } else {
            false
        };

        for node in &changed_nodes {
            let id = self.sources[&node.id];
            let parent = node
                .parent
                .as_ref()
                .and_then(|parent| self.sources.get(parent))
                .copied();
            let children = node
                .children
                .iter()
                .filter_map(|child| self.sources.get(child))
                .copied()
                .collect::<Vec<_>>();
            let flags = interaction.flags_for(&node.id);
            let existing = self.node(id);
            let was_mounted = existing.mounted;
            let old_layout = existing.layout_bounds;
            let old_paint = existing.paint_bounds;
            let old_parent = existing.parent;
            let old_children = existing.children.clone();
            let old_interaction = existing.interaction;
            let next_paint = effective_node_paint_bounds(node);
            let compositing_only = was_mounted
                && compositing_spec_only_changed(&existing.node, node)
                && !tree.has_shadow_ancestor(&node.id);

            if !was_mounted {
                mutations.push(HostMutation::InsertNode {
                    id,
                    source: node.id.clone(),
                    bounds: next_paint,
                });
            } else {
                if old_layout != node.layout_rect || old_paint != next_paint {
                    mutations.push(HostMutation::UpdateProps {
                        id,
                        source: node.id.clone(),
                        kind: HostUpdateKind::Layout,
                        old_bounds: old_paint,
                        new_bounds: next_paint,
                    });
                    if !compositing_only {
                        dirty_scene_sources.insert(node.id.clone());
                    }
                }
                if paint_props_changed(&existing.node, node) {
                    mutations.push(HostMutation::UpdateProps {
                        id,
                        source: node.id.clone(),
                        kind: HostUpdateKind::Paint,
                        old_bounds: old_paint,
                        new_bounds: next_paint,
                    });
                    if compositing_only {
                        compositing_updates.insert(
                            node.id.clone(),
                            node.compositing_layer.expect("compositing layer spec"),
                        );
                    } else {
                        dirty_scene_sources.insert(node.id.clone());
                    }
                }
                if old_interaction != flags {
                    mutations.push(HostMutation::UpdateProps {
                        id,
                        source: node.id.clone(),
                        kind: HostUpdateKind::Interaction,
                        old_bounds: old_paint,
                        new_bounds: next_paint,
                    });
                }
                if old_parent != parent || old_children != children {
                    let bounds = child_structure_damage(
                        self,
                        tree,
                        &old_children,
                        &children,
                        old_paint.union(next_paint),
                    );
                    mutations.push(HostMutation::ReorderChildren {
                        id,
                        source: node.id.clone(),
                        bounds,
                    });
                    dirty_scene_sources.insert(node.id.clone());
                }
            }

            if !was_mounted {
                dirty_scene_sources.insert(node.id.clone());
            }

            let current = self.node_mut(id);
            current.parent = parent;
            current.mounted = true;
            current.children = children;
            current.kind = node.kind;
            current.layout_bounds = node.layout_rect;
            current.paint_bounds = next_paint;
            current.interaction = flags;
            current.node = (*node).clone();
        }
        #[cfg(feature = "diagnostics-timing")]
        let node_patch_ms = elapsed_ms(node_patch_started);
        #[cfg(feature = "diagnostics-timing")]
        let scene_reconcile_started = Instant::now();
        let (scene_mutations, scene, reused_scene_nodes, compiled_scene_nodes, _scene_snapshot_ms) =
            self.reconcile_scene(
                tree,
                dirty_scene_sources,
                compositing_updates,
                order_changed,
                scene_structure_changed,
                invalidations,
            );
        #[cfg(feature = "diagnostics-timing")]
        let scene_reconcile_ms =
            (elapsed_ms(scene_reconcile_started) - _scene_snapshot_ms).max(0.0);
        #[cfg(feature = "diagnostics-timing")]
        let damage_started = Instant::now();
        let damage = self.calculate_damage(viewport, &mutations, invalidations);
        let semantics = self.reconcile_semantics(
            tree,
            semantic_changed,
            semantic_removed,
            interaction.focused.clone(),
            semantic_full,
        );
        #[cfg(feature = "diagnostics-timing")]
        let damage_ms = elapsed_ms(damage_started);
        #[cfg(feature = "diagnostics-timing")]
        let finalize_started = Instant::now();
        self.initialized = true;
        let metrics = HostCommitMetrics {
            host_nodes: self.sources.len(),
            scene_nodes: self.scene.len(),
            visited_host_nodes: changed_nodes.len(),
            compiled_scene_nodes,
            host_mutations: mutations.len(),
            scene_mutations: scene_mutations.len(),
            reused_scene_nodes,
        };
        #[cfg(feature = "diagnostics-timing")]
        let finalize_ms = elapsed_ms(finalize_started);
        HostCommit {
            mutations,
            scene_mutations,
            damage,
            scene,
            metrics,
            semantics,
            #[cfg(feature = "diagnostics-timing")]
            timings: HostCommitTimings {
                change_scan_ms,
                node_patch_ms,
                scene_reconcile_ms,
                scene_snapshot_ms: _scene_snapshot_ms,
                damage_ms,
                finalize_ms,
            },
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
