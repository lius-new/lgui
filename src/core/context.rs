use super::{
    AnimProperty, AnimationRegistry, ComponentId, ComponentState, ComponentStateStore,
    ComponentTree, ContextRegistry, EffectRegistry, HookId, HookSlotKind, HookStateStore,
    InteractionFlags, UiId, UiInteractionState, UiRect, UiScale, UiTaskSpawner, UiUpdateQueue,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct UiRenderContext<'a> {
    interaction: &'a UiInteractionState,
    animations: &'a AnimationRegistry,
    component_states: &'a ComponentStateStore,
    component_tree: &'a ComponentTree,
    contexts: &'a ContextRegistry,
    hook_states: &'a HookStateStore,
    hook_updates: &'a Arc<UiUpdateQueue>,
    task_spawner: Option<&'a UiTaskSpawner>,
    effects: &'a EffectRegistry,
    viewport: UiRect,
    scale: UiScale,
}

impl<'a> UiRenderContext<'a> {
    pub fn new(
        interaction: &'a UiInteractionState,
        animations: &'a AnimationRegistry,
        component_states: &'a ComponentStateStore,
        component_tree: &'a ComponentTree,
        contexts: &'a ContextRegistry,
        hook_states: &'a HookStateStore,
        hook_updates: &'a Arc<UiUpdateQueue>,
        task_spawner: Option<&'a UiTaskSpawner>,
        effects: &'a EffectRegistry,
        viewport: UiRect,
        scale: UiScale,
    ) -> Self {
        Self {
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
        }
    }

    pub fn viewport(&self) -> UiRect {
        self.viewport
    }

    pub fn scale(&self) -> UiScale {
        self.scale
    }

    pub fn interaction_flags(&self, id: &UiId) -> InteractionFlags {
        self.interaction.flags_for(id)
    }

    pub fn animation_value(&self, id: &UiId, property: AnimProperty) -> f32 {
        self.animations.value(id.clone(), property)
    }

    pub fn component_state_mut<T, R>(&self, id: &UiId, f: impl FnOnce(&mut T) -> R) -> R
    where
        T: ComponentState + Clone + Default + 'static,
    {
        self.component_states.with_mut(id, f)
    }

    pub fn preserve_component_state_scope(&self, scope: &UiId) {
        self.component_states.preserve_scope(scope);
    }

    pub fn component_tree(&self) -> &ComponentTree {
        self.component_tree
    }

    pub fn contexts(&self) -> &ContextRegistry {
        self.contexts
    }

    pub fn hook_state<T>(&self, id: HookId, initial: impl FnOnce() -> T) -> T
    where
        T: Clone + 'static,
    {
        self.hook_states.value(id, initial)
    }

    pub fn hook_updates(&self) -> Arc<UiUpdateQueue> {
        Arc::clone(self.hook_updates)
    }

    pub fn task_spawner(&self) -> Option<UiTaskSpawner> {
        self.task_spawner.cloned()
    }

    pub fn effect<D, F, R>(&self, id: HookId, deps: D, effect: F)
    where
        D: Clone + PartialEq + 'static,
        F: FnOnce() -> R + 'static,
        R: super::IntoEffectCleanup,
    {
        self.effects.register(id, deps, effect);
    }

    #[doc(hidden)]
    pub fn element_effect<D, F, R>(&self, parent: ComponentId, owner: UiId, deps: D, effect: F)
    where
        D: Clone + PartialEq + 'static,
        F: FnOnce() -> R + 'static,
        R: super::IntoEffectCleanup,
    {
        let component = self.component_tree.element_effect_child(parent, &owner);
        self.component_tree.begin_component_execution(component);
        self.component_tree
            .record_hook(component, HookSlotKind::Effect);
        self.effects.register(
            HookId::new(component, 0, HookSlotKind::Effect),
            deps,
            effect,
        );
        self.component_tree.finish_component(component);
    }
}
