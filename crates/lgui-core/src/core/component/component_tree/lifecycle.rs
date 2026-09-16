use std::collections::HashSet;

use super::storage::{node_mut, stable_hash, ComponentIdentity};
use super::*;

impl ComponentTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_render(&self) {
        if self.render_active.get() {
            self.abort_render();
        }
        self.render_active.set(true);
        self.render_executed.borrow_mut().clear();
        *self.render_dirty.borrow_mut() = Some(self.dirty.borrow().clone());
        self.render_dirty_flags.borrow_mut().clear();
        for slot in self.slots.borrow_mut().iter_mut() {
            if let Some(node) = slot.node.as_mut() {
                self.render_dirty_flags
                    .borrow_mut()
                    .insert(node.id, (node.dirty, node.descendant_dirty));
                node.seen = false;
                node.render_hook_signature.clear();
                node.render_children.clear();
                node.render_props = None;
                node.render_output = None;
                node.render_finished = false;
            }
        }
        self.metrics.set(ComponentRuntimeMetrics::default());
    }

    pub fn root(&self, id: UiId, type_name: &'static str) -> ComponentId {
        self.enter(ComponentIdentity::Root(id), None, type_name)
    }

    pub fn positioned_child(
        &self,
        parent: ComponentId,
        callsite: u64,
        position: usize,
        type_name: &'static str,
    ) -> ComponentId {
        self.enter(
            ComponentIdentity::Positioned {
                parent,
                callsite,
                position,
            },
            Some(parent),
            type_name,
        )
    }

    pub fn keyed_child(
        &self,
        parent: ComponentId,
        callsite: u64,
        key_hash: u64,
        type_name: &'static str,
    ) -> ComponentId {
        self.enter(
            ComponentIdentity::Keyed {
                parent,
                callsite,
                key_hash,
            },
            Some(parent),
            type_name,
        )
    }

    pub(crate) fn element_effect_child(&self, parent: ComponentId, owner: &UiId) -> ComponentId {
        const ELEMENT_EFFECT_CALLSITE: u64 = 0x52c8_85f4_2920_9a37;
        self.keyed_child(
            parent,
            ELEMENT_EFFECT_CALLSITE,
            stable_hash(owner.as_str()),
            "element effect",
        )
    }

    pub fn record_hook(&self, id: ComponentId, kind: HookSlotKind) {
        let mut slots = self.slots.borrow_mut();
        let node = node_mut(&mut slots, id);
        node.render_hook_signature.push(kind);
    }

    pub fn finish_component(&self, id: ComponentId) {
        let mut slots = self.slots.borrow_mut();
        let node = node_mut(&mut slots, id);
        if node.committed && node.hook_signature != node.render_hook_signature {
            panic!(
                "hook order changed in component `{}` ({id}): previous={:?}, current={:?}",
                node.type_name, node.hook_signature, node.render_hook_signature
            );
        }
        node.render_finished = true;
    }

    pub fn begin_component_execution(&self, id: ComponentId) -> bool {
        let force_children = {
            let mut slots = self.slots.borrow_mut();
            let node = node_mut(&mut slots, id);
            node.render_children.clear();
            node.render_hook_signature.clear();
            node.render_props = None;
            node.render_output = None;
            node.render_finished = false;
            node.dirty || !node.committed
        };
        let mut metrics = self.metrics.get();
        metrics.executed += 1;
        self.metrics.set(metrics);
        self.render_executed.borrow_mut().insert(id);
        force_children
    }

    pub(crate) fn take_executed_in_render(&self) -> HashSet<ComponentId> {
        std::mem::take(&mut *self.render_executed.borrow_mut())
    }

    pub fn can_reuse<P>(&self, id: ComponentId, props: &P) -> bool
    where
        P: PartialEq + 'static,
    {
        self.slots
            .borrow()
            .get(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_ref())
            .is_some_and(|node| {
                node.committed
                    && !node.dirty
                    && !node.descendant_dirty
                    && node.output.is_some()
                    && node
                        .props
                        .as_deref()
                        .and_then(|props| props.downcast_ref::<P>())
                        .is_some_and(|previous| previous == props)
            })
    }

    pub fn commit_output<P>(&self, id: ComponentId, props: P, output: UiElement)
    where
        P: 'static,
    {
        let mut slots = self.slots.borrow_mut();
        let node = node_mut(&mut slots, id);
        node.render_props = Some(Box::new(props));
        node.render_output = Some(output);
    }

    pub fn reuse_output(&self, id: ComponentId) -> UiElement {
        let children = {
            let mut slots = self.slots.borrow_mut();
            let node = node_mut(&mut slots, id);
            node.seen = true;
            node.render_hook_signature = node.hook_signature.clone();
            node.render_children = node.children.clone();
            node.render_finished = true;
            let children = node.children.clone();
            let output = node
                .output
                .clone()
                .unwrap_or_else(|| panic!("component `{id}` has no cached output"));
            (children, output)
        };
        for child in children.0 {
            self.preserve_subtree(child);
        }
        children.1
    }

    pub fn abandon_component(&self, id: ComponentId) {
        let mut slots = self.slots.borrow_mut();
        let Some(node) = slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
        else {
            return;
        };
        node.render_hook_signature.clear();
        node.render_children.clear();
        node.render_props = None;
        node.render_output = None;
        node.render_finished = false;
    }

    pub fn mark_dirty(&self, id: ComponentId) -> bool {
        let mut slots = self.slots.borrow_mut();
        let Some(node) = slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
        else {
            return false;
        };
        node.dirty = true;
        let mut parent = node.parent;
        self.dirty.borrow_mut().insert(id);
        while let Some(parent_id) = parent {
            let parent_node = node_mut(&mut slots, parent_id);
            parent_node.descendant_dirty = true;
            self.dirty.borrow_mut().insert(parent_id);
            parent = parent_node.parent;
        }
        true
    }

    pub fn mark_all_dirty(&self) {
        let mut slots = self.slots.borrow_mut();
        let mut dirty = self.dirty.borrow_mut();
        for (index, slot) in slots.iter_mut().enumerate() {
            if let Some(node) = slot.node.as_mut() {
                node.dirty = true;
                node.descendant_dirty = true;
                dirty.insert(ComponentId {
                    index: index as u32,
                    generation: slot.generation,
                });
            }
        }
    }

    pub fn is_alive(&self, id: ComponentId) -> bool {
        self.slots
            .borrow()
            .get(id.index as usize)
            .is_some_and(|slot| slot.generation == id.generation && slot.node.is_some())
    }

    pub fn is_dirty(&self, id: ComponentId) -> bool {
        self.dirty.borrow().contains(&id)
    }

    pub fn end_render(&self) -> Vec<ComponentId> {
        {
            let mut slots = self.slots.borrow_mut();
            let mut dirty = self.dirty.borrow_mut();
            for slot in slots.iter_mut() {
                let Some(node) = slot.node.as_mut().filter(|node| node.seen) else {
                    continue;
                };
                if node.render_finished {
                    if let Some(props) = node.render_props.take() {
                        node.props = Some(props);
                    }
                    if let Some(output) = node.render_output.take() {
                        node.output = Some(output);
                    }
                    node.hook_signature = std::mem::take(&mut node.render_hook_signature);
                    node.children = std::mem::take(&mut node.render_children);
                    node.committed = true;
                    node.dirty = false;
                    node.descendant_dirty = false;
                    dirty.remove(&node.id);
                }
            }
        }
        let removed: Vec<ComponentId> = self
            .slots
            .borrow()
            .iter()
            .filter_map(|slot| {
                slot.node
                    .as_ref()
                    .filter(|node| !node.seen)
                    .map(|node| node.id)
            })
            .collect();
        for id in removed.iter().copied() {
            self.remove(id);
        }
        self.render_replacements.borrow_mut().clear();
        self.render_dirty.borrow_mut().take();
        self.render_dirty_flags.borrow_mut().clear();
        self.render_active.set(false);
        let mut metrics = self.metrics.get();
        metrics.unmounted += removed.len();
        metrics.dirty = self.dirty.borrow().len();
        self.metrics.set(metrics);
        removed
    }

    pub fn abort_render(&self) {
        let pending: Vec<ComponentId> = self
            .slots
            .borrow()
            .iter()
            .filter_map(|slot| {
                slot.node
                    .as_ref()
                    .filter(|node| !node.committed)
                    .map(|node| node.id)
            })
            .collect();
        for id in pending {
            self.remove(id);
        }
        for (identity, id) in self.render_replacements.borrow_mut().drain(..) {
            if self.is_alive(id) {
                self.identities.borrow_mut().insert(identity, id);
            }
        }
        for slot in self.slots.borrow_mut().iter_mut() {
            if let Some(node) = slot.node.as_mut() {
                node.seen = true;
                node.render_hook_signature.clear();
                node.render_children.clear();
                node.render_props = None;
                node.render_output = None;
                node.render_finished = false;
                if let Some((dirty, descendant_dirty)) =
                    self.render_dirty_flags.borrow().get(&node.id).copied()
                {
                    node.dirty = dirty;
                    node.descendant_dirty = descendant_dirty;
                }
            }
        }
        if let Some(dirty) = self.render_dirty.borrow_mut().take() {
            *self.dirty.borrow_mut() = dirty;
        }
        self.render_dirty_flags.borrow_mut().clear();
        self.render_executed.borrow_mut().clear();
        self.render_active.set(false);
        self.metrics.set(ComponentRuntimeMetrics::default());
    }

    pub fn metrics(&self) -> ComponentRuntimeMetrics {
        self.metrics.get()
    }
}
