use crate::core::*;
impl HostTree {
    pub fn behavior_events(
        &self,
        input: &InputEvent,
        focused: Option<&UiId>,
    ) -> Vec<UiHandlerEvent> {
        match input {
            InputEvent::PointerDown { pointer, button } => self
                .nodes()
                .iter()
                .filter(|n| {
                    self.is_interactive(&n.id)
                        && n.input_event_handlers
                            .iter()
                            .any(|b| b.kind == UiEventKind::OutsidePointer)
                })
                .filter_map(|n| {
                    let off = self.ancestor_content_offset(n);
                    if n.hit_rect.translate(off.0, off.1).contains(pointer.point) {
                        return None;
                    }
                    Some(UiHandlerEvent {
                        target: n.id.clone(),
                        payload: UiEventPayload::OutsidePointer {
                            pointer: *pointer,
                            button: *button,
                        },
                        capture_handlers: n
                            .input_event_handlers
                            .iter()
                            .filter(|b| b.kind == UiEventKind::OutsidePointer && b.capture)
                            .map(|b| b.handler.clone())
                            .collect(),
                        bubble_handlers: n
                            .input_event_handlers
                            .iter()
                            .filter(|b| b.kind == UiEventKind::OutsidePointer && !b.capture)
                            .map(|b| b.handler.clone())
                            .collect(),
                    })
                })
                .collect(),
            InputEvent::Keyboard(key)
                if key.state == KeyState::Down
                    && key.key == LogicalKey::Named(NamedKey::Escape) =>
            {
                let target = self.active_focus_scope_id().or_else(|| focused.cloned());
                let event = target
                    .as_ref()
                    .and_then(|id| self.handler_event(id, UiEventPayload::CloseRequested))
                    .or_else(|| {
                        self.nodes()
                            .iter()
                            .rev()
                            .find(|n| {
                                self.is_interactive(&n.id)
                                    && n.input_event_handlers
                                        .iter()
                                        .any(|b| b.kind == UiEventKind::CloseRequested)
                            })
                            .and_then(|n| self.handler_event(&n.id, UiEventPayload::CloseRequested))
                    });
                event.into_iter().collect()
            }
            _ => Vec::new(),
        }
    }
    pub fn visibility_events(&self, previous: &HostTree) -> Vec<IdentifiedEvent> {
        let mut events = Vec::new();
        for n in self.nodes() {
            if let Some(name) = &n.public_id {
                let visible = self.is_visible(&n.id);
                let old = previous
                    .public_internal_id(name)
                    .map(|id| previous.is_visible(&id));
                if old != Some(visible) {
                    events.push(IdentifiedEvent {
                        node: name.clone(),
                        payload: UiEventPayload::Visibility { visible },
                    });
                }
            }
        }
        for n in previous.nodes() {
            if let Some(name) = &n.public_id
                && self.public_internal_id(name).is_none()
                && previous.is_visible(&n.id)
            {
                events.push(IdentifiedEvent {
                    node: name.clone(),
                    payload: UiEventPayload::Visibility { visible: false },
                });
            }
        }
        events
    }
    pub fn identify_handler_event(&self, event: &UiHandlerEvent) -> Option<IdentifiedEvent> {
        let mut node = self.node(&event.target);
        while let Some(n) = node {
            if let Some(name) = &n.public_id {
                return Some(IdentifiedEvent {
                    node: name.clone(),
                    payload: event.payload.clone(),
                });
            }
            node = n.parent.as_ref().and_then(|id| self.node(id));
        }
        None
    }
}
impl RuntimeOutput {
    pub fn identified_events(&self, tree: &HostTree) -> Vec<IdentifiedEvent> {
        let mut events: Vec<_> = self
            .events
            .iter()
            .flat_map(|e| tree.identified_events(e))
            .collect();
        for e in self
            .handler_events
            .iter()
            .filter_map(|e| tree.identify_handler_event(e))
        {
            if !events.contains(&e) {
                events.push(e);
            }
        }
        events
    }
}
impl Element {
    pub fn on_outside_pointer(
        self,
        handler: impl Fn(&mut UiEventContext, PointerData, PointerButton) + Send + Sync + 'static,
    ) -> Self {
        self.on_event(UiEventKind::OutsidePointer, move |cx, p| {
            if let UiEventPayload::OutsidePointer { pointer, button } = p {
                handler(cx, *pointer, *button);
            }
        })
    }
    pub fn on_close_requested(
        self,
        handler: impl Fn(&mut UiEventContext) + Send + Sync + 'static,
    ) -> Self {
        self.on_event(UiEventKind::CloseRequested, move |cx, _| handler(cx))
    }
}
