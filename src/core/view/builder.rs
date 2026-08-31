use super::{
    AnimationRegistry, ComponentStateStore, ComponentTree, ContextRegistry, EffectRegistry,
    HookStateStore, HostTree, InteractionRole, LayoutSpec, RenderCx, RenderPhase, TextStyle,
    UiElement, UiId, UiInteractionState, UiNode, UiNodeKind, UiRect, UiRenderContext, UiScale,
    UiScope, UiTaskSpawner, UiUpdateQueue, VisualStyle,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub trait RootComponent {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> super::Element;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostProjectionMetrics {
    pub visited_nodes: usize,
    pub reused_component_roots: usize,
}

pub struct HostTreeBuilder {
    tree: HostTree,
    parent_stack: Vec<UiId>,
    retained: bool,
    fresh_owners: HashSet<super::ComponentId>,
    seen_by_owner: HashMap<super::ComponentId, HashSet<UiId>>,
    structure_changed: bool,
    projection_metrics: HostProjectionMetrics,
}

struct RenderTransaction<'a> {
    component_states: &'a ComponentStateStore,
    component_tree: &'a ComponentTree,
    contexts: &'a ContextRegistry,
    hook_states: &'a HookStateStore,
    effects: &'a EffectRegistry,
    committed: bool,
}

impl RenderTransaction<'_> {
    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for RenderTransaction<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        self.component_tree.abort_render();
        self.contexts.abort_render(self.component_tree);
        self.hook_states.abort_render(self.component_tree);
        self.component_states.abort_frame();
        self.effects.abort_frame();
    }
}

impl HostTreeBuilder {
    pub fn new() -> Self {
        Self {
            tree: HostTree::new(),
            parent_stack: Vec::new(),
            retained: false,
            fresh_owners: HashSet::new(),
            seen_by_owner: HashMap::new(),
            structure_changed: false,
            projection_metrics: HostProjectionMetrics::default(),
        }
    }

    pub fn from_retained(tree: HostTree) -> Self {
        Self {
            tree,
            parent_stack: Vec::new(),
            retained: true,
            fresh_owners: HashSet::new(),
            seen_by_owner: HashMap::new(),
            structure_changed: false,
            projection_metrics: HostProjectionMetrics::default(),
        }
    }

    pub fn push(&mut self, mut node: UiNode) -> UiId {
        if node.parent.is_none() {
            node.parent = self.parent_stack.last().cloned();
        }
        let id = node.id.clone();
        self.tree.push(node);
        id
    }

    pub(crate) fn mount_element(&mut self, element: UiElement) -> UiId {
        self.projection_metrics.visited_nodes += 1;
        let (mut node, children, boundary) = element.into_parts();
        if node.parent.is_none() {
            node.parent = self.parent_stack.last().cloned();
        }
        let id = node.id.clone();
        let previous_children = self
            .tree
            .node(&id)
            .map(|current| current.children.clone())
            .unwrap_or_default();
        let retain_children = boundary.is_some_and(|boundary| boundary.retain_children);
        if let Some(boundary) = boundary {
            if !boundary.retain_children {
                self.fresh_owners.insert(boundary.id);
            }
        }
        node.children = previous_children.clone();
        if retain_children {
            self.projection_metrics.reused_component_roots += 1;
        }
        if let Some(owner) = node.component_owner {
            self.seen_by_owner
                .entry(owner)
                .or_default()
                .insert(id.clone());
        }
        if self.retained {
            self.tree.upsert(node);
        } else {
            self.tree.push(node);
        }
        if !retain_children {
            self.parent_stack.push(id.clone());
            let next_children = children
                .iter()
                .cloned()
                .map(|child| self.mount_element(child))
                .collect::<Vec<_>>();
            self.parent_stack.pop();
            self.structure_changed |= previous_children != next_children;
            self.tree.set_children(&id, next_children);
        }
        id
    }

    pub fn node(&mut self, id: UiId, kind: UiNodeKind, rect: UiRect) -> UiId {
        self.push(UiNode::new(id, kind, rect))
    }

    pub fn interactive(
        &mut self,
        id: UiId,
        kind: UiNodeKind,
        rect: UiRect,
        interaction: InteractionRole,
    ) -> UiId {
        self.push(UiNode::new(id, kind, rect).interaction(interaction))
    }

    pub fn styled(&mut self, id: UiId, kind: UiNodeKind, rect: UiRect, style: VisualStyle) -> UiId {
        self.push(UiNode::new(id, kind, rect).style(style))
    }

    pub fn text(
        &mut self,
        id: UiId,
        rect: UiRect,
        text: impl Into<std::borrow::Cow<'static, str>>,
        style: TextStyle,
    ) -> UiId {
        self.push(UiNode::new(id, UiNodeKind::Text, rect).text(text, style))
    }

    pub fn laid_out(
        &mut self,
        id: UiId,
        kind: UiNodeKind,
        rect: UiRect,
        layout: LayoutSpec,
    ) -> UiId {
        self.push(UiNode::new(id, kind, rect).layout(layout))
    }

    pub fn in_phase(
        &mut self,
        id: UiId,
        kind: UiNodeKind,
        rect: UiRect,
        phase: RenderPhase,
    ) -> UiId {
        self.push(UiNode::new(id, kind, rect).render_phase(phase))
    }

    pub fn with_parent<T>(&mut self, id: UiId, build: impl FnOnce(&mut Self) -> T) -> T {
        self.parent_stack.push(id);
        let result = build(self);
        self.parent_stack.pop();
        result
    }

    pub fn element(&mut self, element: UiElement) -> UiId {
        element.mount(self)
    }

    pub fn mount(
        &mut self,
        component: impl RootComponent,
        viewport: UiRect,
        interaction: &UiInteractionState,
        animations: &AnimationRegistry,
        component_states: &ComponentStateStore,
        component_tree: &ComponentTree,
        contexts: &ContextRegistry,
        hook_states: &HookStateStore,
        hook_updates: &Arc<UiUpdateQueue>,
        task_spawner: Option<&UiTaskSpawner>,
        effects: &EffectRegistry,
        scale: UiScale,
    ) -> UiId {
        component_states.begin_frame();
        component_tree.begin_render();
        contexts.begin_render();
        hook_states.begin_render();
        effects.begin_frame();
        let mut transaction = RenderTransaction {
            component_states,
            component_tree,
            contexts,
            hook_states,
            effects,
            committed: false,
        };
        let scope = UiScope::new("ui");
        let context = UiRenderContext::new(
            interaction,
            animations,
            component_states,
            component_tree,
            contexts,
            hook_states,
            hook_updates,
            task_spawner,
            effects,
            viewport,
            scale,
        );
        let root = {
            let mut cx = RenderCx::new(&scope, &context);
            let _current_context = context.contexts().enter_current(cx.component_id());
            let view = component.render_root(&mut cx);
            let root = cx.component_id();
            self.element(
                cx.compile(view)
                    .claim_component_owner(root)
                    .component_boundary(root),
            )
        };
        component_states.end_frame();
        component_tree.end_render();
        self.fresh_owners
            .extend(component_tree.take_executed_in_render());
        for owner in self.fresh_owners.clone() {
            let keep = self.seen_by_owner.get(&owner).cloned().unwrap_or_default();
            self.structure_changed |= self.tree.retain_owner_nodes(owner, &keep);
        }
        self.structure_changed |= self.tree.prune_dead_component_owners(component_tree);
        if self.structure_changed {
            self.tree.reorder_by_hierarchy();
        }
        contexts.end_render(component_tree);
        hook_states.end_render(component_tree);
        effects.end_frame(component_tree);
        transaction.commit();
        root
    }

    pub fn finish(self) -> HostTree {
        self.tree
    }

    pub fn projection_metrics(&self) -> HostProjectionMetrics {
        self.projection_metrics
    }
}

impl Default for HostTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "builder/tests.rs"]
mod tests;
