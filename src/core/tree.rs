use super::{
    compile_scene, ActionId, ComponentId, ComponentTree, EventPolicy, InteractionRole, Point,
    Scene, UiAction, UiActionHandler, UiEvent, UiEventHandler, UiEventKind, UiEventPayload,
    UiHandlerEvent, UiId, UiNode, UiRect,
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
    nodes: Vec<UiNode>,
    owners: std::collections::HashMap<ComponentId, std::collections::HashSet<UiId>>,
    projection_changes: ProjectionChanges,
}

#[derive(Clone, Default)]
pub(crate) struct ProjectionChanges {
    pub changed: std::collections::HashSet<UiId>,
    pub removed: std::collections::HashSet<UiId>,
    pub structure_changed: bool,
}

impl HostTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            owners: std::collections::HashMap::new(),
            projection_changes: ProjectionChanges::default(),
        }
    }

    pub fn push(&mut self, node: UiNode) {
        self.projection_changes.changed.insert(node.id.clone());
        self.projection_changes.structure_changed = true;
        if let Some(parent_id) = node.parent.as_ref() {
            if let Some(parent) = self.node_mut(parent_id) {
                parent.children.push(node.id.clone());
            }
        }
        if let Some(owner) = node.component_owner {
            self.owners
                .entry(owner)
                .or_default()
                .insert(node.id.clone());
        }
        self.nodes.push(node);
    }

    pub(crate) fn upsert(&mut self, node: UiNode) {
        self.projection_changes.changed.insert(node.id.clone());
        if let Some(index) = self.nodes.iter().position(|current| current.id == node.id) {
            let previous_owner = self.nodes[index].component_owner;
            if previous_owner != node.component_owner {
                if let Some(owner) = previous_owner {
                    if let Some(ids) = self.owners.get_mut(&owner) {
                        ids.remove(&node.id);
                    }
                }
            }
            if let Some(owner) = node.component_owner {
                self.owners
                    .entry(owner)
                    .or_default()
                    .insert(node.id.clone());
            }
            self.nodes[index] = node;
        } else {
            self.push(node);
        }
    }

    pub(crate) fn attach_child(&mut self, parent: &UiId, child: UiId) {
        let changed_parent = {
            let Some(parent) = self.node_mut(parent) else {
                return;
            };
            if parent.children.contains(&child) {
                None
            } else {
                parent.children.push(child);
                Some(parent.id.clone())
            }
        };
        if let Some(parent) = changed_parent {
            self.projection_changes.changed.insert(parent);
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
        self.nodes.retain(|node| !remove.contains(&node.id));
        self.projection_changes
            .removed
            .extend(remove.iter().cloned());
        self.projection_changes.structure_changed = true;
        for node in &mut self.nodes {
            node.children.retain(|child| !remove.contains(child));
            if node
                .parent
                .as_ref()
                .is_some_and(|parent| remove.contains(parent))
            {
                node.parent = None;
            }
        }
        self.owners.retain(|_, ids| {
            ids.retain(|id| !remove.contains(id));
            !ids.is_empty()
        });
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
            nodes: &std::collections::HashMap<UiId, UiNode>,
            seen: &mut std::collections::HashSet<UiId>,
            ordered: &mut Vec<UiNode>,
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
    }

    pub(crate) fn take_projection_changes(&mut self) -> ProjectionChanges {
        std::mem::take(&mut self.projection_changes)
    }

    pub fn nodes(&self) -> &[UiNode] {
        &self.nodes
    }

    pub(crate) fn nodes_mut(&mut self) -> &mut [UiNode] {
        &mut self.nodes
    }

    pub fn node(&self, id: &UiId) -> Option<&UiNode> {
        self.nodes.iter().find(|node| &node.id == id)
    }

    pub fn node_mut(&mut self, id: &UiId) -> Option<&mut UiNode> {
        self.nodes.iter_mut().find(|node| &node.id == id)
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
            UiEvent::Wheel { hit, delta_y } => self
                .handler_event(&hit.id, UiEventPayload::Wheel { delta_y: *delta_y })
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
            UiEvent::ImeUpdated { target, text } => self
                .handler_event(
                    target,
                    UiEventPayload::CompositionUpdate { text: text.clone() },
                )
                .into_iter()
                .collect(),
            UiEvent::ImeEnded { target } => self
                .handler_event(target, UiEventPayload::CompositionEnd)
                .into_iter()
                .collect(),
            UiEvent::Backspace { target } => self
                .handler_event(
                    target,
                    UiEventPayload::KeyDown {
                        key: super::KeyCode::Backspace,
                        modifiers: super::KeyModifiers::default(),
                    },
                )
                .into_iter()
                .collect(),
            UiEvent::KeyDown {
                target,
                key,
                modifiers,
            } => self
                .handler_event(
                    target,
                    UiEventPayload::KeyDown {
                        key: *key,
                        modifiers: *modifiers,
                    },
                )
                .into_iter()
                .collect(),
            UiEvent::PointerPressed { hit, point } => self
                .handler_event(&hit.id, UiEventPayload::PointerDown { point: *point })
                .into_iter()
                .collect(),
            UiEvent::PointerMoved { hit, point } | UiEvent::PointerDragged { hit, point } => self
                .handler_event(&hit.id, UiEventPayload::PointerMove { point: *point })
                .into_iter()
                .collect(),
            UiEvent::PointerReleased { hit, point } => self
                .handler_event(&hit.id, UiEventPayload::PointerUp { point: *point })
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
            | UiEvent::PointerLeft { .. } => Vec::new(),
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

    pub(crate) fn ancestor_content_offset(&self, node: &UiNode) -> (i32, i32) {
        let mut x = 0;
        let mut y = 0;
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

fn node_dirty_bounds(node: &UiNode) -> UiRect {
    let (x, y) = node.animation_outset;
    node.paint_bounds.inflate(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{RenderPhase, UiNodeKind};

    #[test]
    fn popup_hit_testing_wins_over_later_siblings_and_ancestor_clips() {
        let clip_id = UiId::new("clip");
        let popup_id = UiId::new("popup");
        let sibling_id = UiId::new("sibling");
        let target = UiRect::new(20, 20, 40, 40);
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(clip_id.clone(), UiNodeKind::Clip, UiRect::new(0, 0, 10, 10)).clip(
                UiRect::new(0, 0, 10, 10),
                0,
                0,
            ),
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
            .hit_test(Point::new(30, 30))
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
                UiRect::new(0, 0, 400, 64),
            )
            .interaction(InteractionRole::WindowDragRegion)
            .event_policy(crate::core::EventPolicy::NONE),
        );
        tree.push(
            UiNode::new(
                button_id.clone(),
                UiNodeKind::Button,
                UiRect::new(350, 0, 400, 64),
            )
            .parent(drag_id.clone())
            .interaction(InteractionRole::Button),
        );

        let drag = tree
            .hit_test(Point::new(100, 32))
            .expect("titlebar background should be draggable");
        assert_eq!(drag.id, drag_id);
        assert_eq!(drag.interaction, InteractionRole::WindowDragRegion);

        let button = tree
            .hit_test(Point::new(375, 32))
            .expect("titlebar button should remain interactive");
        assert_eq!(button.id, button_id);
        assert_eq!(button.interaction, InteractionRole::Button);
    }
}
