use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fmt,
};

use super::{UiElement, UiId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId {
    index: u32,
    generation: u32,
}

impl ComponentId {
    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.index, self.generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HookSlotKind {
    Stable,
    State,
    Effect,
    Context,
    Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HookId {
    component: ComponentId,
    index: u32,
    kind: HookSlotKind,
}

impl HookId {
    pub const fn new(component: ComponentId, index: usize, kind: HookSlotKind) -> Self {
        Self {
            component,
            index: index as u32,
            kind,
        }
    }

    pub const fn component(self) -> ComponentId {
        self.component
    }

    pub const fn index(self) -> usize {
        self.index as usize
    }

    pub const fn kind(self) -> HookSlotKind {
        self.kind
    }
}

impl fmt::Display for HookId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "component {} hook {} ({:?})",
            self.component, self.index, self.kind
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ComponentIdentity {
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

struct ComponentNode {
    id: ComponentId,
    identity: ComponentIdentity,
    parent: Option<ComponentId>,
    type_name: &'static str,
    children: Vec<ComponentId>,
    render_children: Vec<ComponentId>,
    hook_signature: Vec<HookSlotKind>,
    render_hook_signature: Vec<HookSlotKind>,
    props: Option<Box<dyn Any>>,
    output: Option<UiElement>,
    render_props: Option<Box<dyn Any>>,
    render_output: Option<UiElement>,
    render_finished: bool,
    committed: bool,
    seen: bool,
    dirty: bool,
    descendant_dirty: bool,
}

struct ComponentSlot {
    generation: u32,
    node: Option<ComponentNode>,
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
    slots: RefCell<Vec<ComponentSlot>>,
    free: RefCell<Vec<u32>>,
    identities: RefCell<HashMap<ComponentIdentity, ComponentId>>,
    dirty: RefCell<HashSet<ComponentId>>,
    metrics: Cell<ComponentRuntimeMetrics>,
    render_active: Cell<bool>,
    render_executed: RefCell<HashSet<ComponentId>>,
    render_replacements: RefCell<Vec<(ComponentIdentity, ComponentId)>>,
    render_dirty: RefCell<Option<HashSet<ComponentId>>>,
    render_dirty_flags: RefCell<HashMap<ComponentId, (bool, bool)>>,
}

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

    fn enter(
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

    fn remove(&self, id: ComponentId) {
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

    fn preserve_subtree(&self, id: ComponentId) {
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

fn node_mut(slots: &mut [ComponentSlot], id: ComponentId) -> &mut ComponentNode {
    slots
        .get_mut(id.index as usize)
        .filter(|slot| slot.generation == id.generation)
        .and_then(|slot| slot.node.as_mut())
        .unwrap_or_else(|| panic!("stale component id `{id}`"))
}

fn stable_hash(value: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    value.as_bytes().iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

fn attach_child(slots: &mut [ComponentSlot], parent: ComponentId, child: ComponentId) {
    let parent = node_mut(slots, parent);
    if !parent.render_children.contains(&child) {
        parent.render_children.push(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_children_keep_identity_when_order_changes() {
        let tree = ComponentTree::new();
        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        let first = tree.keyed_child(root, 1, 10, "row");
        let second = tree.keyed_child(root, 1, 20, "row");
        tree.finish_component(first);
        tree.finish_component(second);
        tree.finish_component(root);
        tree.end_render();

        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        let second_again = tree.keyed_child(root, 1, 20, "row");
        let first_again = tree.keyed_child(root, 1, 10, "row");

        assert_eq!(first, first_again);
        assert_eq!(second, second_again);
    }

    #[test]
    fn changing_component_type_replaces_the_instance_without_borrow_reentrancy() {
        let tree = ComponentTree::new();
        tree.begin_render();
        let first = tree.root(UiId::owned("root"), "first");
        tree.finish_component(first);
        tree.end_render();

        tree.begin_render();
        let second = tree.root(UiId::owned("root"), "second");
        tree.finish_component(second);
        tree.end_render();

        assert_ne!(first, second);
        assert!(!tree.is_alive(first));
        assert!(tree.is_alive(second));
    }

    #[test]
    fn abort_render_restores_replaced_component_identity() {
        let tree = ComponentTree::new();
        tree.begin_render();
        let first = tree.root(UiId::owned("root"), "first");
        tree.finish_component(first);
        tree.end_render();

        tree.begin_render();
        let replacement = tree.root(UiId::owned("root"), "replacement");
        assert_ne!(first, replacement);
        tree.abort_render();

        tree.begin_render();
        let restored = tree.root(UiId::owned("root"), "first");
        assert_eq!(restored, first);
        assert!(!tree.is_alive(replacement));
    }

    #[test]
    fn abort_render_preserves_committed_props_output_and_dirty_flags() {
        let tree = ComponentTree::new();
        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        tree.begin_component_execution(root);
        tree.commit_output(
            root,
            1_u32,
            UiElement::group(
                UiId::owned("committed"),
                super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
            ),
        );
        tree.finish_component(root);
        tree.end_render();

        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        tree.begin_component_execution(root);
        tree.commit_output(
            root,
            2_u32,
            UiElement::group(
                UiId::owned("abandoned"),
                super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
            ),
        );
        tree.finish_component(root);
        tree.abort_render();

        assert!(tree.can_reuse(root, &1_u32));
        assert!(!tree.can_reuse(root, &2_u32));
        assert_eq!(
            tree.reuse_output(root).into_parts().0.id.as_str(),
            "committed"
        );

        assert!(tree.mark_dirty(root));
        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        tree.finish_component(root);
        tree.abort_render();
        assert!(tree.is_dirty(root));
    }

    #[test]
    #[should_panic(expected = "hook order changed")]
    fn hook_order_changes_are_rejected() {
        let tree = ComponentTree::new();
        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        tree.record_hook(root, HookSlotKind::State);
        tree.finish_component(root);
        tree.end_render();

        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        tree.record_hook(root, HookSlotKind::Effect);
        tree.finish_component(root);
    }

    #[test]
    fn dirty_branch_executes_while_an_unchanged_sibling_reuses_its_output() {
        let tree = ComponentTree::new();
        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        tree.begin_component_execution(root);
        let first = tree.positioned_child(root, 1, 0, "child");
        tree.begin_component_execution(first);
        tree.commit_output(
            first,
            1_u32,
            UiElement::group(
                UiId::owned("first"),
                super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
            ),
        );
        tree.finish_component(first);
        let second = tree.positioned_child(root, 1, 1, "child");
        tree.begin_component_execution(second);
        tree.commit_output(
            second,
            2_u32,
            UiElement::group(
                UiId::owned("second"),
                super::super::UiRect::new(1.0, 0.0, 1.0, 1.0),
            ),
        );
        tree.finish_component(second);
        tree.finish_component(root);
        tree.end_render();

        assert!(tree.mark_dirty(first));
        tree.begin_render();
        let root = tree.root(UiId::owned("root"), "root");
        assert!(!tree.begin_component_execution(root));
        let first_again = tree.positioned_child(root, 1, 0, "child");
        assert!(!tree.can_reuse(first_again, &1_u32));
        tree.begin_component_execution(first_again);
        tree.commit_output(
            first_again,
            1_u32,
            UiElement::group(
                UiId::owned("first"),
                super::super::UiRect::new(0.0, 0.0, 1.0, 1.0),
            ),
        );
        tree.finish_component(first_again);
        let second_again = tree.positioned_child(root, 1, 1, "child");
        assert!(tree.can_reuse(second_again, &2_u32));
        tree.reuse_output(second_again);
        tree.finish_component(root);
        tree.end_render();

        assert_eq!(tree.metrics().executed, 2);
    }
}
