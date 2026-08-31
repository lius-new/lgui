use super::{
    compile_scene, ActionId, ComponentId, ComponentTree, CompositingLayerSpec, EventPolicy,
    InteractionRole, Point, Scene, UiAction, UiActionHandler, UiEvent, UiEventHandler, UiEventKind,
    UiEventPayload, UiHandlerEvent, UiId, UiNode, UiRect,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone)]
pub struct HitResult {
    pub id: UiId,
    pub rect: UiRect,
    pub interaction: InteractionRole,
    pub policy: EventPolicy,
    pub action: Option<UiAction>,
    pub action_target: Option<UiId>,
    pub capture_handlers: Vec<UiEventHandler>,
    pub bubble_handlers: Vec<UiEventHandler>,
    pub click_handler: Option<UiEventHandler>,
}

impl std::fmt::Debug for HitResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HitResult")
            .field("id", &self.id)
            .field("rect", &self.rect)
            .field("interaction", &self.interaction)
            .field("policy", &self.policy)
            .field("action", &self.action)
            .field("action_target", &self.action_target)
            .field("capture_handlers", &self.capture_handlers.len())
            .field("bubble_handlers", &self.bubble_handlers.len())
            .field("has_click_handler", &self.click_handler.is_some())
            .finish()
    }
}

impl PartialEq for HitResult {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.rect == other.rect
            && self.interaction == other.interaction
            && self.policy == other.policy
            && self.action == other.action
            && self.action_target == other.action_target
            && self.capture_handlers.len() == other.capture_handlers.len()
            && self.bubble_handlers.len() == other.bubble_handlers.len()
            && self.click_handler.is_some() == other.click_handler.is_some()
    }
}

impl Eq for HitResult {}

#[derive(Clone, Default)]
pub struct HostTree {
    nodes: Vec<Arc<UiNode>>,
    node_indices: Arc<HashMap<UiId, usize>>,
    owners: Arc<HashMap<ComponentId, HashSet<UiId>>>,
    projection_changes: ProjectionChanges,
}

#[derive(Clone, Default)]
pub(crate) struct ProjectionChanges {
    pub changed: std::collections::HashSet<UiId>,
    pub removed: std::collections::HashSet<UiId>,
    pub structure_changed: bool,
    pub(crate) animation_sync: std::collections::HashSet<UiId>,
    pub(crate) focus_sync: bool,
}

impl HostTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            node_indices: Arc::new(HashMap::new()),
            owners: Arc::new(HashMap::new()),
            projection_changes: ProjectionChanges::default(),
        }
    }

    pub fn push(&mut self, node: UiNode) {
        self.projection_changes.changed.insert(node.id.clone());
        self.projection_changes.structure_changed = true;
        self.mark_runtime_sync_for_insert(&node);
        if let Some(parent_id) = node.parent.as_ref() {
            if let Some(parent) = self.node_mut(parent_id) {
                parent.children.push(node.id.clone());
            }
        }
        if let Some(owner) = node.component_owner {
            Arc::make_mut(&mut self.owners)
                .entry(owner)
                .or_default()
                .insert(node.id.clone());
        }
        let index = self.nodes.len();
        Arc::make_mut(&mut self.node_indices).insert(node.id.clone(), index);
        self.nodes.push(Arc::new(node));
    }

    pub(crate) fn upsert(&mut self, node: UiNode) {
        if let Some(index) = self.node_indices.get(&node.id).copied() {
            if !self.nodes[index].projection_eq(&node) {
                self.projection_changes.changed.insert(node.id.clone());
            }
            let previous = Arc::clone(&self.nodes[index]);
            self.mark_runtime_sync_for_update(&previous, &node);
            let previous_owner = self.nodes[index].component_owner;
            if previous_owner != node.component_owner {
                if let Some(owner) = previous_owner {
                    if let Some(ids) = Arc::make_mut(&mut self.owners).get_mut(&owner) {
                        ids.remove(&node.id);
                    }
                }
                if let Some(owner) = node.component_owner {
                    Arc::make_mut(&mut self.owners)
                        .entry(owner)
                        .or_default()
                        .insert(node.id.clone());
                }
            }
            self.nodes[index] = Arc::new(node);
        } else {
            self.push(node);
        }
    }

    pub(crate) fn set_children(&mut self, parent: &UiId, children: Vec<UiId>) {
        let changed = {
            let Some(parent) = self.node_mut(parent) else {
                return;
            };
            if parent.children == children {
                false
            } else {
                parent.children = children;
                true
            }
        };
        if changed {
            self.projection_changes.changed.insert(parent.clone());
            self.projection_changes.structure_changed = true;
        }
    }

    pub(crate) fn retain_owner_nodes(
        &mut self,
        owner: ComponentId,
        keep: &std::collections::HashSet<UiId>,
    ) -> bool {
        let remove = self
            .owners
            .get(&owner)
            .into_iter()
            .flatten()
            .filter(|id| !keep.contains(*id))
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        self.remove_ids(&remove)
    }

    pub(crate) fn prune_dead_component_owners(&mut self, components: &ComponentTree) -> bool {
        let remove = self
            .owners
            .keys()
            .copied()
            .filter(|owner| !components.is_alive(*owner))
            .flat_map(|owner| self.owners.get(&owner).into_iter().flatten().cloned())
            .collect::<std::collections::HashSet<_>>();
        self.remove_ids(&remove)
    }

    fn remove_ids(&mut self, remove: &std::collections::HashSet<UiId>) -> bool {
        if remove.is_empty() {
            return false;
        }
        for id in remove {
            if let Some(index) = self.node_indices.get(id).copied() {
                let node = Arc::clone(&self.nodes[index]);
                self.mark_runtime_sync_for_remove(&node);
            }
        }
        self.nodes.retain(|node| !remove.contains(&node.id));
        self.projection_changes
            .removed
            .extend(remove.iter().cloned());
        self.projection_changes.structure_changed = true;
        for node in &mut self.nodes {
            let children_changed = node.children.iter().any(|child| remove.contains(child));
            let parent_changed = node
                .parent
                .as_ref()
                .is_some_and(|parent| remove.contains(parent));
            if !children_changed && !parent_changed {
                continue;
            }
            let node = Arc::make_mut(node);
            if children_changed {
                node.children.retain(|child| !remove.contains(child));
            }
            if parent_changed {
                node.parent = None;
            }
        }
        Arc::make_mut(&mut self.owners).retain(|_, ids| {
            ids.retain(|id| !remove.contains(id));
            !ids.is_empty()
        });
        self.rebuild_node_indices();
        true
    }

    pub(crate) fn reorder_by_hierarchy(&mut self) {
        let previous_order = self
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        let nodes = self
            .nodes
            .drain(..)
            .map(|node| (node.id.clone(), node))
            .collect::<std::collections::HashMap<_, _>>();
        let roots = previous_order
            .iter()
            .filter(|id| nodes.get(*id).is_some_and(|node| node.parent.is_none()))
            .cloned()
            .collect::<Vec<_>>();
        let mut ordered = Vec::with_capacity(nodes.len());
        let mut seen = std::collections::HashSet::new();
        fn append(
            id: &UiId,
            nodes: &std::collections::HashMap<UiId, Arc<UiNode>>,
            seen: &mut std::collections::HashSet<UiId>,
            ordered: &mut Vec<Arc<UiNode>>,
        ) {
            if !seen.insert(id.clone()) {
                return;
            }
            let Some(node) = nodes.get(id) else {
                return;
            };
            ordered.push(node.clone());
            for child in &node.children {
                append(child, nodes, seen, ordered);
            }
        }
        for root in roots {
            append(&root, &nodes, &mut seen, &mut ordered);
        }
        for id in nodes.keys() {
            append(id, &nodes, &mut seen, &mut ordered);
        }
        self.nodes = ordered;
        self.rebuild_node_indices();
    }

    pub(crate) fn take_projection_changes(&mut self) -> ProjectionChanges {
        std::mem::take(&mut self.projection_changes)
    }

    pub fn nodes(&self) -> &[Arc<UiNode>] {
        &self.nodes
    }

    pub fn node(&self, id: &UiId) -> Option<&UiNode> {
        self.node_indices
            .get(id)
            .and_then(|index| self.nodes.get(*index))
            .map(Arc::as_ref)
    }

    pub fn node_mut(&mut self, id: &UiId) -> Option<&mut UiNode> {
        let index = self.node_indices.get(id).copied()?;
        self.nodes.get_mut(index).map(Arc::make_mut)
    }

    pub(crate) fn changed_nodes(&self, changed: &HashSet<UiId>) -> Vec<&UiNode> {
        let mut indices = changed
            .iter()
            .filter_map(|id| self.node_indices.get(id).copied())
            .collect::<Vec<_>>();
        indices.sort_unstable();
        indices
            .into_iter()
            .filter_map(|index| self.nodes.get(index).map(Arc::as_ref))
            .collect()
    }

    pub(crate) fn animation_sync_ids(&self) -> impl Iterator<Item = &UiId> {
        self.projection_changes.animation_sync.iter()
    }

    pub(crate) fn needs_focus_sync(&self) -> bool {
        self.projection_changes.focus_sync
    }

    pub(crate) fn update_compositing_layer(
        &mut self,
        id: &UiId,
        spec: CompositingLayerSpec,
    ) -> Option<UiRect> {
        let index = self.node_indices.get(id).copied()?;
        let node = Arc::make_mut(&mut self.nodes[index]);
        let previous = node.compositing_layer?;
        if previous == spec {
            return None;
        }

        let visible_bounds = |spec: CompositingLayerSpec| {
            (spec.opacity > 0).then(|| {
                spec.transform
                    .transformed_bounds(node.layout_rect)
                    .inflate(node.animation_outset.0, node.animation_outset.1)
            })
        };
        let old_bounds = visible_bounds(previous);
        let new_bounds = visible_bounds(spec);
        node.compositing_layer = Some(spec);
        self.projection_changes.changed.insert(id.clone());

        match (old_bounds, new_bounds) {
            (Some(old), Some(new)) => Some(old.union(new)),
            (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
            (None, None) => None,
        }
    }

    fn rebuild_node_indices(&mut self) {
        self.node_indices = Arc::new(
            self.nodes
                .iter()
                .enumerate()
                .map(|(index, node)| (node.id.clone(), index))
                .collect(),
        );
    }

    fn mark_runtime_sync_for_insert(&mut self, node: &UiNode) {
        if node_needs_animation_sync(node) {
            self.projection_changes
                .animation_sync
                .insert(node.id.clone());
        }
        self.projection_changes.focus_sync |= node_affects_focus(node);
    }

    fn mark_runtime_sync_for_update(&mut self, previous: &UiNode, next: &UiNode) {
        if previous.animation_bindings != next.animation_bindings
            || previous.animation_targets != next.animation_targets
        {
            self.projection_changes
                .animation_sync
                .insert(next.id.clone());
        }
        self.projection_changes.focus_sync |= !focus_projection_eq(previous, next);
    }

    fn mark_runtime_sync_for_remove(&mut self, node: &UiNode) {
        if node_needs_animation_sync(node) {
            self.projection_changes
                .animation_sync
                .insert(node.id.clone());
        }
        self.projection_changes.focus_sync |= node_affects_focus(node);
    }

    pub fn action_handler(&self, target: &UiId, id: &ActionId) -> Option<UiActionHandler> {
        self.node(target)?
            .action_handlers
            .iter()
            .find(|binding| &binding.id == id)
            .map(|binding| binding.handler.clone())
    }

    pub fn hit_test(&self, point: Point) -> Option<HitResult> {
        let node = self.frontmost_node_at(point, |node| {
            node.interaction != InteractionRole::None
                || node.click_handler.is_some()
                || node.click_capture_handler.is_some()
                || node.input_event_handlers.iter().any(|binding| {
                    matches!(
                        binding.kind,
                        UiEventKind::Click
                            | UiEventKind::PointerDown
                            | UiEventKind::PointerMove
                            | UiEventKind::PointerUp
                    )
                })
        })?;
        Some(self.hit_for_node(node, node.click_action.clone()))
    }

    pub fn wheel_hit_test(&self, point: Point) -> Option<HitResult> {
        let node = self.frontmost_node_at(point, |node| {
            node.wheel_action.is_some()
                || node
                    .input_event_handlers
                    .iter()
                    .any(|binding| binding.kind == UiEventKind::Wheel)
        })?;
        let mut hit = self.hit_for_node(node, node.wheel_action.clone());
        hit.click_handler = None;
        hit.capture_handlers.clear();
        hit.bubble_handlers.clear();
        Some(hit)
    }

    pub fn focusable_hits(&self) -> Vec<HitResult> {
        self.nodes
            .iter()
            .filter(|node| node.event_policy.focus)
            .map(|node| self.hit_for_node(node, None))
            .collect()
    }

    pub fn focusable_hits_in_scope(&self, scope_id: &UiId) -> Vec<HitResult> {
        self.nodes
            .iter()
            .filter(|node| {
                node.event_policy.focus
                    && (&node.id == scope_id || self.node_is_descendant_of(&node.id, scope_id))
            })
            .map(|node| self.hit_for_node(node, None))
            .collect()
    }

    pub fn focusable_hit(&self, id: &UiId) -> Option<HitResult> {
        self.nodes
            .iter()
            .find(|node| &node.id == id && node.event_policy.focus)
            .map(|node| self.hit_for_node(node, None))
    }

    pub fn hit_for_id(&self, id: &UiId) -> Option<HitResult> {
        self.node(id).map(|node| self.hit_for_node(node, None))
    }

    pub fn handler_events(&self, event: &UiEvent) -> Vec<UiHandlerEvent> {
        match event {
            UiEvent::Clicked(hit) => self
                .handler_event(&hit.id, UiEventPayload::Click)
                .into_iter()
                .collect(),
            UiEvent::Wheel { hit, delta } => self
                .handler_event(&hit.id, UiEventPayload::Wheel { delta: *delta })
                .into_iter()
                .collect(),
            UiEvent::TextInput { target, text } => self
                .handler_event(target, UiEventPayload::Input { text: text.clone() })
                .into_iter()
                .collect(),
            UiEvent::ImeStarted { target } => self
                .handler_event(target, UiEventPayload::CompositionStart)
                .into_iter()
                .collect(),
            UiEvent::ImeUpdated {
                target,
                text,
                cursor,
            } => self
                .handler_event(
                    target,
                    UiEventPayload::CompositionUpdate {
                        text: text.clone(),
                        cursor: cursor.clone(),
                    },
                )
                .into_iter()
                .collect(),
            UiEvent::ImeEnded { target } => self
                .handler_event(target, UiEventPayload::CompositionEnd)
                .into_iter()
                .collect(),
            UiEvent::Keyboard { target, event } => self
                .handler_event(
                    target,
                    UiEventPayload::Keyboard {
                        event: event.clone(),
                    },
                )
                .into_iter()
                .collect(),
            UiEvent::PointerPressed { hit, pointer } => self
                .handler_event(&hit.id, UiEventPayload::PointerDown { pointer: *pointer })
                .into_iter()
                .collect(),
            UiEvent::PointerMoved { hit, pointer } | UiEvent::PointerDragged { hit, pointer } => {
                self.handler_event(&hit.id, UiEventPayload::PointerMove { pointer: *pointer })
                    .into_iter()
                    .collect()
            }
            UiEvent::PointerReleased { hit, pointer } => self
                .handler_event(&hit.id, UiEventPayload::PointerUp { pointer: *pointer })
                .into_iter()
                .collect(),
            UiEvent::FocusChanged { previous, current } => {
                let mut events = previous
                    .as_ref()
                    .and_then(|id| self.handler_event(id, UiEventPayload::Blur))
                    .into_iter()
                    .collect::<Vec<_>>();
                events.extend(
                    current
                        .as_ref()
                        .and_then(|hit| self.handler_event(&hit.id, UiEventPayload::Focus)),
                );
                events
            }
            UiEvent::HoverChanged { .. }
            | UiEvent::PressedChanged { .. }
            | UiEvent::PointerLeft { .. }
            | UiEvent::SemanticValue { .. }
            | UiEvent::SemanticAction { .. } => Vec::new(),
        }
    }

    pub fn handler_event(&self, target: &UiId, payload: UiEventPayload) -> Option<UiHandlerEvent> {
        let node = self.node(target)?;
        let kind = payload.kind();
        let mut lineage = vec![node];
        let mut parent = node.parent.as_ref();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                break;
            };
            lineage.push(parent_node);
            parent = parent_node.parent.as_ref();
        }
        let capture_handlers = lineage
            .iter()
            .rev()
            .flat_map(|node| &node.input_event_handlers)
            .filter(|binding| binding.capture && binding.kind == kind)
            .map(|binding| binding.handler.clone())
            .collect::<Vec<_>>();
        let bubble_handlers = lineage
            .iter()
            .flat_map(|node| &node.input_event_handlers)
            .filter(|binding| !binding.capture && binding.kind == kind)
            .map(|binding| binding.handler.clone())
            .collect::<Vec<_>>();
        (!capture_handlers.is_empty() || !bubble_handlers.is_empty()).then(|| UiHandlerEvent {
            target: target.clone(),
            payload,
            capture_handlers,
            bubble_handlers,
        })
    }

    pub fn auto_focus_hit(&self) -> Option<HitResult> {
        self.nodes
            .iter()
            .find(|node| node.auto_focus && node.event_policy.focus)
            .map(|node| self.hit_for_node(node, None))
    }

    pub fn focus_scope_ids(&self) -> Vec<UiId> {
        self.nodes
            .iter()
            .filter(|node| node.focus_scope)
            .map(|node| node.id.clone())
            .collect()
    }

    pub fn active_focus_scope_id(&self) -> Option<UiId> {
        self.nodes
            .iter()
            .rev()
            .find(|node| node.focus_scope && node.render_phase == super::RenderPhase::Popup)
            .or_else(|| self.nodes.iter().rev().find(|node| node.focus_scope))
            .map(|node| node.id.clone())
    }

    pub fn focusable_hit_in_scope(&self, id: &UiId, scope_id: &UiId) -> Option<HitResult> {
        self.node(id)
            .filter(|node| {
                node.event_policy.focus
                    && (&node.id == scope_id || self.node_is_descendant_of(&node.id, scope_id))
            })
            .map(|node| self.hit_for_node(node, None))
    }

    pub fn auto_focus_hit_in_scope(&self, scope_id: &UiId) -> Option<HitResult> {
        self.nodes
            .iter()
            .find(|node| {
                node.auto_focus
                    && node.event_policy.focus
                    && self.node_is_descendant_of(&node.id, scope_id)
            })
            .map(|node| self.hit_for_node(node, None))
    }

    fn node_is_descendant_of(&self, id: &UiId, ancestor_id: &UiId) -> bool {
        let mut parent = self.node(id).and_then(|node| node.parent.as_ref());
        while let Some(parent_id) = parent {
            if parent_id == ancestor_id {
                return true;
            }
            parent = self.node(parent_id).and_then(|node| node.parent.as_ref());
        }
        false
    }

    fn frontmost_node_at(
        &self,
        point: Point,
        accepts: impl Fn(&UiNode) -> bool,
    ) -> Option<&UiNode> {
        let contains = |node: &UiNode| {
            let offset = self.ancestor_content_offset(node);
            accepts(node)
                && node.hit_rect.translate(offset.0, offset.1).contains(point)
                && self.node_visible_at_point(node, point)
        };
        self.nodes
            .iter()
            .rev()
            .find(|node| node.render_phase == super::RenderPhase::Popup && contains(node))
            .or_else(|| {
                self.nodes
                    .iter()
                    .rev()
                    .find(|node| node.render_phase != super::RenderPhase::Popup && contains(node))
            })
            .map(Arc::as_ref)
    }

    fn node_visible_at_point(&self, node: &UiNode, point: Point) -> bool {
        if node.render_phase == super::RenderPhase::Popup {
            return true;
        }
        let mut parent = node.parent.as_ref();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                return true;
            };
            let offset = self.ancestor_content_offset(parent_node);
            if parent_node
                .clip_rect
                .is_some_and(|clip| !clip.translate(offset.0, offset.1).contains(point))
            {
                return false;
            }
            parent = parent_node.parent.as_ref();
        }
        true
    }

    pub(crate) fn ancestor_content_offset(&self, node: &UiNode) -> (f32, f32) {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut parent = node.parent.as_ref();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                break;
            };
            x += parent_node.content_offset.0;
            y += parent_node.content_offset.1;
            parent = parent_node.parent.as_ref();
        }
        (x, y)
    }

    fn hit_for_node(&self, node: &UiNode, action: Option<UiAction>) -> HitResult {
        let offset = self.ancestor_content_offset(node);
        let mut lineage = vec![node];
        let mut parent = node.parent.as_ref();
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else {
                break;
            };
            lineage.push(parent_node);
            parent = parent_node.parent.as_ref();
        }
        let capture_handlers = lineage
            .iter()
            .rev()
            .filter_map(|node| node.click_capture_handler.clone())
            .collect();
        let bubble_handlers = lineage
            .iter()
            .filter_map(|node| node.click_handler.clone())
            .collect();
        HitResult {
            id: node.id.clone(),
            rect: node.hit_rect.translate(offset.0, offset.1),
            interaction: node.interaction,
            policy: node.event_policy,
            action,
            action_target: node.action_target.clone(),
            capture_handlers,
            bubble_handlers,
            click_handler: node.click_handler.clone(),
        }
    }

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

fn node_needs_animation_sync(node: &UiNode) -> bool {
    !node.animation_bindings.is_empty() || !node.animation_targets.is_empty()
}

fn node_affects_focus(node: &UiNode) -> bool {
    node.event_policy.focus || node.auto_focus || node.focus_scope
}

fn focus_projection_eq(previous: &UiNode, next: &UiNode) -> bool {
    previous.parent == next.parent
        && previous.render_phase == next.render_phase
        && previous.event_policy.focus == next.event_policy.focus
        && previous.auto_focus == next.auto_focus
        && previous.focus_scope == next.focus_scope
}

fn node_dirty_bounds(node: &UiNode) -> UiRect {
    let (x, y) = node.animation_outset;
    node.paint_bounds.inflate(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{AnimProperty, AnimationBinding, RenderPhase, UiNodeKind, VisualStyle};

    #[test]
    fn retained_clone_shares_unchanged_nodes_and_detaches_only_updated_nodes() {
        let first_id = UiId::new("first");
        let second_id = UiId::new("second");
        let mut original = HostTree::new();
        original.push(UiNode::new(
            first_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 10.0, 10.0),
        ));
        original.push(UiNode::new(
            second_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(10.0, 0.0, 20.0, 10.0),
        ));
        original.take_projection_changes();

        let mut next = original.clone();
        assert!(Arc::ptr_eq(&original.nodes[0], &next.nodes[0]));
        assert!(Arc::ptr_eq(&original.nodes[1], &next.nodes[1]));

        next.upsert(
            UiNode::new(
                first_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 10.0, 10.0),
            )
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );

        assert!(!Arc::ptr_eq(&original.nodes[0], &next.nodes[0]));
        assert!(Arc::ptr_eq(&original.nodes[1], &next.nodes[1]));
        assert_ne!(
            original.node(&first_id).unwrap().style,
            next.node(&first_id).unwrap().style
        );
        assert!(next.node(&second_id).is_some());
    }

    #[test]
    fn runtime_sync_changes_track_only_relevant_nodes() {
        let plain_id = UiId::new("plain");
        let animated_id = UiId::new("animated");
        let mut tree = HostTree::new();
        tree.push(UiNode::new(
            plain_id.clone(),
            UiNodeKind::Panel,
            UiRect::new(0.0, 0.0, 10.0, 10.0),
        ));
        tree.push(
            UiNode::new(
                animated_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(10.0, 0.0, 20.0, 10.0),
            )
            .animation(AnimationBinding::new(AnimProperty::Opacity, 0.0, 1.0))
            .animation_target(AnimProperty::Opacity, false),
        );
        assert_eq!(tree.animation_sync_ids().count(), 1);
        tree.take_projection_changes();

        tree.upsert(
            UiNode::new(
                plain_id,
                UiNodeKind::Panel,
                UiRect::new(0.0, 0.0, 10.0, 10.0),
            )
            .style(VisualStyle::filled(crate::core::Color::WHITE)),
        );

        assert_eq!(tree.animation_sync_ids().count(), 0);
        assert!(!tree.needs_focus_sync());
    }

    #[test]
    fn popup_hit_testing_wins_over_later_siblings_and_ancestor_clips() {
        let clip_id = UiId::new("clip");
        let popup_id = UiId::new("popup");
        let sibling_id = UiId::new("sibling");
        let target = UiRect::new(20.0, 20.0, 40.0, 40.0);
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                clip_id.clone(),
                UiNodeKind::Clip,
                UiRect::new(0.0, 0.0, 10.0, 10.0),
            )
            .clip(UiRect::new(0.0, 0.0, 10.0, 10.0), 0.0, 0.0),
        );
        tree.push(
            UiNode::new(popup_id.clone(), UiNodeKind::Button, target)
                .parent(clip_id)
                .interaction(InteractionRole::Button)
                .render_phase(RenderPhase::Popup),
        );
        tree.push(
            UiNode::new(sibling_id, UiNodeKind::Button, target)
                .interaction(InteractionRole::Button)
                .render_phase(RenderPhase::Overlay),
        );

        let hit = tree
            .hit_test(Point::new(30.0, 30.0))
            .expect("popup should be hittable outside its ancestor clip");
        assert_eq!(hit.id, popup_id);
    }

    #[test]
    fn interactive_titlebar_children_take_precedence_over_the_drag_region() {
        let drag_id = UiId::new("titlebar");
        let button_id = UiId::new("close");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                drag_id.clone(),
                UiNodeKind::Group,
                UiRect::new(0.0, 0.0, 400.0, 64.0),
            )
            .interaction(InteractionRole::WindowDragRegion)
            .event_policy(crate::core::EventPolicy::NONE),
        );
        tree.push(
            UiNode::new(
                button_id.clone(),
                UiNodeKind::Button,
                UiRect::new(350.0, 0.0, 400.0, 64.0),
            )
            .parent(drag_id.clone())
            .interaction(InteractionRole::Button),
        );

        let drag = tree
            .hit_test(Point::new(100.0, 32.0))
            .expect("titlebar background should be draggable");
        assert_eq!(drag.id, drag_id);
        assert_eq!(drag.interaction, InteractionRole::WindowDragRegion);

        let button = tree
            .hit_test(Point::new(375.0, 32.0))
            .expect("titlebar button should remain interactive");
        assert_eq!(button.id, button_id);
        assert_eq!(button.interaction, InteractionRole::Button);
    }
}
