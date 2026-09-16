use super::*;

impl HostRuntime {
    pub(super) fn allocate(&mut self, source: UiId) -> HostNodeId {
        let index = self.free.pop().unwrap_or(self.slots.len() as u32);
        if index as usize == self.slots.len() {
            self.slots.push(HostSlot {
                generation: 0,
                node: None,
            });
        }
        let generation = self.slots[index as usize].generation;
        let id = HostNodeId { index, generation };
        self.slots[index as usize].node = Some(HostNode {
            source: source.clone(),
            mounted: false,
            parent: None,
            children: Vec::new(),
            kind: UiNodeKind::Root,
            layout_bounds: UiRect::new(0.0, 0.0, 0.0, 0.0),
            paint_bounds: UiRect::new(0.0, 0.0, 0.0, 0.0),
            interaction: InteractionFlags::default(),
            node: UiNode::new(source, UiNodeKind::Root, UiRect::new(0.0, 0.0, 0.0, 0.0)),
        });
        id
    }

    pub(super) fn remove(&mut self, id: HostNodeId) {
        let slot = &mut self.slots[id.index as usize];
        let Some(node) = slot.node.take() else {
            return;
        };
        self.sources.remove(&node.source);
        self.scene.remove(&id);
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
    }

    pub(super) fn node(&self, id: HostNodeId) -> &HostNode {
        self.try_node(id)
            .unwrap_or_else(|| panic!("stale host node id `{id}`"))
    }

    pub(super) fn try_node(&self, id: HostNodeId) -> Option<&HostNode> {
        self.slots
            .get(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_ref())
    }

    pub(super) fn node_mut(&mut self, id: HostNodeId) -> &mut HostNode {
        self.slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
            .unwrap_or_else(|| panic!("stale host node id `{id}`"))
    }
}
