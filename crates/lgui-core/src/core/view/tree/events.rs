use super::*;

impl HostTree {
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

    /// Resolves the cursor icon for the frontmost node under `point`, walking
    /// up the ancestor chain to the nearest node that requests a non-default
    /// cursor. Returns `None` when no node under the point requests one.
    pub fn cursor_at(&self, point: Point) -> Option<CursorIcon> {
        let mut current = self.frontmost_node_at(point, |_| true);
        while let Some(node) = current {
            if node.cursor != CursorIcon::Default {
                return Some(node.cursor);
            }
            current = node.parent.as_ref().and_then(|id| self.node(id));
        }
        None
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
            .find(|node| node.focus_scope && node.render_phase == RenderPhase::Popup)
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
            .find(|node| node.render_phase == RenderPhase::Popup && contains(node))
            .or_else(|| {
                self.nodes
                    .iter()
                    .rev()
                    .find(|node| node.render_phase != RenderPhase::Popup && contains(node))
            })
            .map(Arc::as_ref)
    }

    fn node_visible_at_point(&self, node: &UiNode, point: Point) -> bool {
        if node.render_phase == RenderPhase::Popup {
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
            cursor: node.cursor,
            policy: node.event_policy,
            action,
            action_target: node.action_target.clone(),
            capture_handlers,
            bubble_handlers,
            click_handler: node.click_handler.clone(),
        }
    }
}
