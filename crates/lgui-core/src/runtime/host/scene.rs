use super::*;

impl HostRuntime {
    pub(super) fn scene_owner_source(&self, id: HostNodeId) -> Option<UiId> {
        let mut current = id;
        let mut owner = (is_scene_drawable(self.node(current).kind)
            || self.node(current).node.shadow.is_some())
        .then_some(current);
        while let Some(parent) = self.node(current).parent {
            let current_node = self.node(current);
            let parent_node = self.node(parent);
            if current_node.node.render_phase == crate::core::RenderPhase::Popup
                && parent_node.node.render_phase != crate::core::RenderPhase::Popup
            {
                break;
            }
            if is_scene_container(parent_node.kind) || parent_node.node.shadow.is_some() {
                owner = Some(parent);
            }
            current = parent;
        }
        owner.map(|owner| self.node(owner).source.clone())
    }

    pub(super) fn reconcile_scene(
        &mut self,
        tree: &HostTree,
        dirty_sources: HashSet<UiId>,
        compositing_updates: HashMap<UiId, CompositingLayerSpec>,
        host_order_changed: bool,
        scene_structure_changed: bool,
        invalidations: &mut InvalidationSet,
    ) -> (Vec<SceneMutation>, Scene, usize, usize, f32) {
        let retained_order = !scene_structure_changed
            && self.initialized
            && self
                .scene_order
                .iter()
                .all(|id| self.try_node(*id).is_some());
        let root_ids = if retained_order {
            self.scene_order.clone()
        } else {
            scene_root_ids(tree)
                .iter()
                .filter_map(|source| self.sources.get(source))
                .copied()
                .collect::<Vec<_>>()
        };
        let removed = if retained_order {
            Vec::new()
        } else {
            let live = root_ids.iter().copied().collect::<HashSet<_>>();
            self.scene
                .keys()
                .filter(|id| !live.contains(id))
                .copied()
                .collect::<Vec<_>>()
        };
        let mut mutations = removed
            .iter()
            .copied()
            .map(SceneMutation::Remove)
            .collect::<Vec<_>>();
        for id in removed {
            if let Some(scene) = self.scene.remove(&id) {
                if let Some(bounds) = shadow_bounds(&scene.commands) {
                    invalidations.invalidate_rect(bounds);
                }
            }
        }

        let mut dirty_roots = HashSet::new();
        for source in dirty_sources {
            if let Some(owner) = scene_owner_source_for_tree(tree, &source) {
                if let Some(id) = self.sources.get(&owner) {
                    dirty_roots.insert(*id);
                }
            }
        }

        let mut fast_updated = HashSet::new();
        let mut fast_specs = Vec::new();
        for (source, spec) in compositing_updates {
            let Some(owner_source) = scene_owner_source_for_tree(tree, &source) else {
                continue;
            };
            let Some(owner_id) = self.sources.get(&owner_source).copied() else {
                continue;
            };
            if dirty_roots.contains(&owner_id) {
                continue;
            }
            let Some(scene) = self.scene.get_mut(&owner_id) else {
                continue;
            };
            let old_shadow_bounds = shadow_bounds(&scene.commands);
            if patch_compositing_layer_spec(&mut scene.commands, &source, spec) {
                if let Some(bounds) = old_shadow_bounds {
                    invalidations.invalidate_rect(bounds);
                }
                if let Some(bounds) = shadow_bounds(&scene.commands) {
                    invalidations.invalidate_rect(bounds);
                }
                scene.signature = command_signature(&scene.commands);
                if fast_updated.insert(owner_id) {
                    mutations.push(SceneMutation::Update(owner_id));
                }
                fast_specs.push((source, spec));
            }
        }

        let mut reused = 0;
        let mut compiled = 0;
        for id in &root_ids {
            if self.scene.contains_key(id) && !dirty_roots.contains(id) {
                if !fast_updated.contains(id) {
                    reused += 1;
                }
                continue;
            }
            let source = self.node(*id).source.clone();
            let compiled_scene = compile_scene_root(tree, &source);
            compiled += 1;
            let commands = compiled_scene.commands().to_vec();
            let signature = command_signature(&commands);
            let previous = self.scene.get(id);
            if previous.is_none_or(|scene| scene.signature != signature) {
                if let Some(bounds) = previous.and_then(|scene| shadow_bounds(&scene.commands)) {
                    invalidations.invalidate_rect(bounds);
                }
                if let Some(bounds) = shadow_bounds(&commands) {
                    invalidations.invalidate_rect(bounds);
                }
            }
            match self.scene.get_mut(id) {
                Some(scene) if scene.signature == signature => reused += 1,
                Some(scene) => {
                    scene.signature = signature;
                    scene.commands = commands;
                    mutations.push(SceneMutation::Update(*id));
                }
                None => {
                    self.scene.insert(
                        *id,
                        SceneNode {
                            signature,
                            commands,
                        },
                    );
                    mutations.push(SceneMutation::Insert(*id));
                }
            }
        }
        let next_scene_order = root_ids;
        if host_order_changed || self.scene_order != next_scene_order {
            mutations.push(SceneMutation::Reorder);
        }
        self.scene_order = next_scene_order;
        let structural_change = mutations.iter().any(|mutation| {
            matches!(
                mutation,
                SceneMutation::Insert(_) | SceneMutation::Remove(_) | SceneMutation::Reorder
            )
        }) || self.scene_ranges.len() != self.scene_order.len();
        if structural_change {
            let mut commands = Vec::new();
            let mut ranges = HashMap::new();
            for id in &self.scene_order {
                let start = commands.len();
                if let Some(scene_node) = self.scene.get(id) {
                    commands.extend(scene_node.commands.iter().cloned());
                }
                ranges.insert(*id, (start, commands.len()));
            }
            self.composed_scene.replace_all(commands);
            self.scene_ranges = ranges;
        } else {
            let updated = mutations
                .iter()
                .filter_map(|mutation| match mutation {
                    SceneMutation::Update(id) if !fast_updated.contains(id) => Some(*id),
                    _ => None,
                })
                .collect::<HashSet<_>>();
            if !updated.is_empty() {
                let previous_ranges = std::mem::take(&mut self.scene_ranges);
                let mut next_ranges = HashMap::with_capacity(previous_ranges.len());
                let mut offset = 0_isize;
                for id in &self.scene_order {
                    let (old_start, old_end) = previous_ranges[id];
                    let start = (old_start as isize + offset) as usize;
                    let end = (old_end as isize + offset) as usize;
                    if updated.contains(id) {
                        let commands = self
                            .scene
                            .get(id)
                            .map(|node| node.commands.clone())
                            .unwrap_or_default();
                        let next_end = start + commands.len();
                        self.composed_scene.replace_range(start..end, commands);
                        offset += next_end as isize - end as isize;
                        next_ranges.insert(*id, (start, next_end));
                    } else {
                        next_ranges.insert(*id, (start, end));
                    }
                }
                self.scene_ranges = next_ranges;
            }
        }
        for (source, spec) in fast_specs {
            let patched = self
                .composed_scene
                .patch_compositing_layer_spec(&source, spec);
            debug_assert!(patched, "retained compositing command must exist");
        }
        #[cfg(feature = "diagnostics-timing")]
        let snapshot_started = Instant::now();
        let scene = self.composed_scene.clone();
        #[cfg(feature = "diagnostics-timing")]
        let snapshot_ms = elapsed_ms(snapshot_started);
        #[cfg(not(feature = "diagnostics-timing"))]
        let snapshot_ms = 0.0;
        (mutations, scene, reused, compiled, snapshot_ms)
    }
}

fn command_signature(commands: &[ScenePrimitive]) -> u64 {
    commands.iter().fold(0, |signature, command| {
        signature.rotate_left(7) ^ command.signature()
    })
}

pub(super) fn shadow_bounds(commands: &[ScenePrimitive]) -> Option<UiRect> {
    if !commands.iter().any(ScenePrimitive::contains_shadow) {
        return None;
    }
    commands
        .iter()
        .map(ScenePrimitive::paint_bounds)
        .reduce(UiRect::union)
}

fn scene_owner_source_for_tree(tree: &HostTree, source: &UiId) -> Option<UiId> {
    let mut current = tree.node(source)?;
    let mut owner =
        (is_scene_drawable(current.kind) || current.shadow.is_some()).then(|| current.id.clone());
    while let Some(parent_id) = current.parent.as_ref() {
        let Some(parent) = tree.node(parent_id) else {
            break;
        };
        if current.render_phase == crate::core::RenderPhase::Popup
            && parent.render_phase != crate::core::RenderPhase::Popup
        {
            break;
        }
        if is_scene_container(parent.kind) || parent.shadow.is_some() {
            owner = Some(parent.id.clone());
        }
        current = parent;
    }
    owner
}

fn is_scene_container(kind: UiNodeKind) -> bool {
    matches!(
        kind,
        UiNodeKind::CompositingLayer
            | UiNodeKind::StaticLayer
            | UiNodeKind::ScrollRaster
            | UiNodeKind::ContentBlur
            | UiNodeKind::Clip
            | UiNodeKind::ClipPath
    )
}

fn is_scene_drawable(kind: UiNodeKind) -> bool {
    !matches!(kind, UiNodeKind::Root | UiNodeKind::Group)
}
