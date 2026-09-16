use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum ComponentIdentity {
    Root(UiId),
    Positioned {
        parent: ComponentId,
        callsite: u64,
        position: usize,
    },
    Keyed {
        parent: ComponentId,
        callsite: u64,
        key_hash: u64,
    },
}

pub(super) struct ComponentNode {
    pub(super) id: ComponentId,
    pub(super) identity: ComponentIdentity,
    pub(super) parent: Option<ComponentId>,
    pub(super) type_name: &'static str,
    pub(super) children: Vec<ComponentId>,
    pub(super) render_children: Vec<ComponentId>,
    pub(super) hook_signature: Vec<HookSlotKind>,
    pub(super) render_hook_signature: Vec<HookSlotKind>,
    pub(super) props: Option<Box<dyn Any>>,
    pub(super) output: Option<UiElement>,
    pub(super) render_props: Option<Box<dyn Any>>,
    pub(super) render_output: Option<UiElement>,
    pub(super) render_finished: bool,
    pub(super) committed: bool,
    pub(super) seen: bool,
    pub(super) dirty: bool,
    pub(super) descendant_dirty: bool,
}

pub(super) struct ComponentSlot {
    pub(super) generation: u32,
    pub(super) node: Option<ComponentNode>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComponentRuntimeMetrics {
    pub mounted: usize,
    pub unmounted: usize,
    pub executed: usize,
    pub dirty: usize,
}

#[derive(Default)]
pub struct ComponentTree {
    pub(super) slots: RefCell<Vec<ComponentSlot>>,
    pub(super) free: RefCell<Vec<u32>>,
    pub(super) identities: RefCell<HashMap<ComponentIdentity, ComponentId>>,
    pub(super) dirty: RefCell<HashSet<ComponentId>>,
    pub(super) metrics: Cell<ComponentRuntimeMetrics>,
    pub(super) render_active: Cell<bool>,
    pub(super) render_executed: RefCell<HashSet<ComponentId>>,
    pub(super) render_replacements: RefCell<Vec<(ComponentIdentity, ComponentId)>>,
    pub(super) render_dirty: RefCell<Option<HashSet<ComponentId>>>,
    pub(super) render_dirty_flags: RefCell<HashMap<ComponentId, (bool, bool)>>,
}

impl ComponentTree {
    pub fn clear(&self) {
        self.slots.borrow_mut().clear();
        self.free.borrow_mut().clear();
        self.identities.borrow_mut().clear();
        self.dirty.borrow_mut().clear();
        self.render_executed.borrow_mut().clear();
        self.render_replacements.borrow_mut().clear();
        self.render_dirty.borrow_mut().take();
        self.render_dirty_flags.borrow_mut().clear();
        self.render_active.set(false);
        self.metrics.set(ComponentRuntimeMetrics::default());
    }

    pub(super) fn enter(
        &self,
        identity: ComponentIdentity,
        parent: Option<ComponentId>,
        type_name: &'static str,
    ) -> ComponentId {
        let existing = { self.identities.borrow().get(&identity).copied() };
        if let Some(existing) = existing {
            let same_type = self
                .slots
                .borrow()
                .get(existing.index as usize)
                .filter(|slot| slot.generation == existing.generation)
                .and_then(|slot| slot.node.as_ref())
                .is_some_and(|node| node.type_name == type_name);
            if same_type {
                let mut slots = self.slots.borrow_mut();
                let node = node_mut(&mut slots, existing);
                node.seen = true;
                node.render_children.clear();
                node.render_hook_signature.clear();
                if let Some(parent) = parent {
                    attach_child(&mut slots, parent, existing);
                }
                return existing;
            }
            self.identities.borrow_mut().remove(&identity);
            self.render_replacements
                .borrow_mut()
                .push((identity.clone(), existing));
        }

        let id = self.allocate(identity.clone(), parent, type_name);
        self.identities.borrow_mut().insert(identity, id);
        if let Some(parent) = parent {
            attach_child(&mut self.slots.borrow_mut(), parent, id);
        }
        let mut metrics = self.metrics.get();
        metrics.mounted += 1;
        self.metrics.set(metrics);
        id
    }

    fn allocate(
        &self,
        identity: ComponentIdentity,
        parent: Option<ComponentId>,
        type_name: &'static str,
    ) -> ComponentId {
        let mut slots = self.slots.borrow_mut();
        let index = self.free.borrow_mut().pop().unwrap_or(slots.len() as u32);
        if index as usize == slots.len() {
            slots.push(ComponentSlot {
                generation: 0,
                node: None,
            });
        }
        let generation = slots[index as usize].generation;
        let id = ComponentId { index, generation };
        slots[index as usize].node = Some(ComponentNode {
            id,
            identity,
            parent,
            type_name,
            children: Vec::new(),
            render_children: Vec::new(),
            hook_signature: Vec::new(),
            render_hook_signature: Vec::new(),
            props: None,
            output: None,
            render_props: None,
            render_output: None,
            render_finished: false,
            committed: false,
            seen: true,
            dirty: true,
            descendant_dirty: false,
        });
        self.dirty.borrow_mut().insert(id);
        id
    }

    pub(super) fn remove(&self, id: ComponentId) {
        let children = {
            let slots = self.slots.borrow();
            let Some(node) = slots
                .get(id.index as usize)
                .filter(|slot| slot.generation == id.generation)
                .and_then(|slot| slot.node.as_ref())
            else {
                return;
            };
            let mut children = node.children.clone();
            for child in &node.render_children {
                if !children.contains(child) {
                    children.push(*child);
                }
            }
            children
        };
        for child in children {
            self.remove(child);
        }
        let mut slots = self.slots.borrow_mut();
        let Some(slot) = slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
        else {
            return;
        };
        let Some(node) = slot.node.take() else {
            return;
        };
        let mut identities = self.identities.borrow_mut();
        if identities.get(&node.identity) == Some(&id) {
            identities.remove(&node.identity);
        }
        self.dirty.borrow_mut().remove(&id);
        slot.generation = slot.generation.wrapping_add(1);
        self.free.borrow_mut().push(id.index);
    }

    pub(super) fn preserve_subtree(&self, id: ComponentId) {
        let children = {
            let mut slots = self.slots.borrow_mut();
            let node = node_mut(&mut slots, id);
            node.seen = true;
            node.render_hook_signature = node.hook_signature.clone();
            node.render_children = node.children.clone();
            node.render_finished = true;
            node.children.clone()
        };
        for child in children {
            self.preserve_subtree(child);
        }
    }
}

pub(super) fn node_mut(slots: &mut [ComponentSlot], id: ComponentId) -> &mut ComponentNode {
    slots
        .get_mut(id.index as usize)
        .filter(|slot| slot.generation == id.generation)
        .and_then(|slot| slot.node.as_mut())
        .unwrap_or_else(|| panic!("stale component id `{id}`"))
}

pub(super) fn stable_hash(value: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    value.as_bytes().iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

pub(super) fn attach_child(slots: &mut [ComponentSlot], parent: ComponentId, child: ComponentId) {
    let parent = node_mut(slots, parent);
    if !parent.render_children.contains(&child) {
        parent.render_children.push(child);
    }
}
