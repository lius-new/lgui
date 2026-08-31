use super::{
    compile_scene, ActionId, ComponentId, ComponentTree, CompositingLayerSpec, EventPolicy,
    InteractionRole, Point, RenderPhase, Scene, UiAction, UiActionHandler, UiEvent, UiEventHandler,
    UiEventKind, UiEventPayload, UiHandlerEvent, UiId, UiNode, UiRect,
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

impl HostTree {
    /// Returns a conservative estimate of retained tree storage, including shared node payloads.
    #[cfg(any(
        test,
        feature = "backend-winit",
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        all(feature = "backend-win32", feature = "renderer-skia")
    ))]
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(
                self.nodes
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Arc<UiNode>>()),
            )
            .saturating_add(
                self.nodes
                    .iter()
                    .map(|node| node.estimated_bytes())
                    .sum::<usize>(),
            )
    }
}

#[derive(Clone, Default)]
pub(crate) struct ProjectionChanges {
    pub changed: std::collections::HashSet<UiId>,
    pub removed: std::collections::HashSet<UiId>,
    pub structure_changed: bool,
    pub(crate) animation_sync: std::collections::HashSet<UiId>,
    pub(crate) focus_sync: bool,
}

#[path = "tree/events.rs"]
mod events;
#[path = "tree/mutation.rs"]
mod mutation;
#[path = "tree/scene.rs"]
mod scene;

#[cfg(test)]
#[path = "tree/tests.rs"]
mod tests;
