use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostNodeId {
    pub(super) index: u32,
    pub(super) generation: u32,
}

impl fmt::Display for HostNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.index, self.generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostUpdateKind {
    Layout,
    Paint,
    Interaction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostMutation {
    InsertNode {
        id: HostNodeId,
        source: UiId,
        bounds: UiRect,
    },
    RemoveNode {
        id: HostNodeId,
        source: UiId,
        old_bounds: UiRect,
    },
    UpdateProps {
        id: HostNodeId,
        source: UiId,
        kind: HostUpdateKind,
        old_bounds: UiRect,
        new_bounds: UiRect,
    },
    ReorderChildren {
        id: HostNodeId,
        source: UiId,
        bounds: UiRect,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneMutation {
    Insert(HostNodeId),
    Update(HostNodeId),
    Remove(HostNodeId),
    Reorder,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCommitMetrics {
    pub host_nodes: usize,
    pub scene_nodes: usize,
    pub visited_host_nodes: usize,
    pub compiled_scene_nodes: usize,
    pub host_mutations: usize,
    pub scene_mutations: usize,
    pub reused_scene_nodes: usize,
}

#[cfg(feature = "diagnostics-timing")]
#[derive(Clone, Copy, Debug, Default)]
#[doc(hidden)]
pub struct HostCommitTimings {
    pub change_scan_ms: f32,
    pub node_patch_ms: f32,
    pub scene_reconcile_ms: f32,
    pub scene_snapshot_ms: f32,
    pub damage_ms: f32,
    pub finalize_ms: f32,
}

#[derive(Clone, Debug)]
pub struct HostCommit {
    pub mutations: Vec<HostMutation>,
    pub scene_mutations: Vec<SceneMutation>,
    pub damage: DamageReport,
    pub scene: Scene,
    pub metrics: HostCommitMetrics,
    pub semantics: SemanticUpdate,
    #[cfg(feature = "diagnostics-timing")]
    #[doc(hidden)]
    pub timings: HostCommitTimings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageReason {
    FirstCommit,
    Explicit,
    Insert,
    Remove,
    Layout,
    Paint,
    Interaction,
    Structure,
    Clean,
}

#[derive(Clone, Debug)]
pub struct DamageDetail {
    pub reason: DamageReason,
    pub node_id: Option<UiId>,
    pub old_bounds: Option<UiRect>,
    pub new_bounds: Option<UiRect>,
    pub rects: Vec<UiRect>,
}

#[derive(Clone, Debug)]
pub struct DamageReport {
    pub dirty: DirtyRegionSet,
    pub reasons: Vec<DamageReason>,
    pub details: Vec<DamageDetail>,
}

pub(super) struct HostNode {
    pub(super) source: UiId,
    pub(super) mounted: bool,
    pub(super) parent: Option<HostNodeId>,
    pub(super) children: Vec<HostNodeId>,
    pub(super) kind: UiNodeKind,
    pub(super) layout_bounds: UiRect,
    pub(super) paint_bounds: UiRect,
    pub(super) interaction: InteractionFlags,
    pub(super) node: UiNode,
}

pub(super) struct HostSlot {
    pub(super) generation: u32,
    pub(super) node: Option<HostNode>,
}

#[derive(Clone)]
pub(super) struct SceneNode {
    pub(super) signature: u64,
    pub(super) commands: Vec<ScenePrimitive>,
}

#[derive(Default)]
pub struct HostRuntime {
    pub(super) slots: Vec<HostSlot>,
    pub(super) free: Vec<u32>,
    pub(super) sources: HashMap<UiId, HostNodeId>,
    pub(super) paint_order: Vec<HostNodeId>,
    pub(super) scene: HashMap<HostNodeId, SceneNode>,
    pub(super) scene_order: Vec<HostNodeId>,
    pub(super) scene_ranges: HashMap<HostNodeId, (usize, usize)>,
    pub(super) composed_scene: Scene,
    pub(super) semantics: HashMap<UiId, SemanticNode>,
    pub(super) initialized: bool,
}

impl HostRuntime {
    #[cfg(any(test, feature = "backend-winit", feature = "backend-win32"))]
    pub(crate) fn estimated_bytes(&self) -> usize {
        let host_nodes = self
            .slots
            .iter()
            .filter_map(|slot| slot.node.as_ref())
            .map(|node| {
                std::mem::size_of::<HostNode>()
                    .saturating_add(
                        node.children
                            .capacity()
                            .saturating_mul(std::mem::size_of::<HostNodeId>()),
                    )
                    .saturating_add(node.node.estimated_bytes())
            })
            .sum::<usize>();
        let scene_nodes = self
            .scene
            .values()
            .map(|node| crate::core::estimate_scene_commands_bytes(&node.commands))
            .sum::<usize>();
        std::mem::size_of::<Self>()
            .saturating_add(host_nodes)
            .saturating_add(scene_nodes)
            .saturating_add(self.composed_scene.estimated_bytes())
    }
}
