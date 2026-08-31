use std::collections::{HashMap, HashSet};
use std::fmt;
#[cfg(feature = "diagnostics-timing")]
use std::time::Instant;

use crate::{
    core::{
        compile_scene_root, patch_compositing_layer_spec, scene_root_ids, CompositingLayerSpec,
        HostTree, InteractionFlags, Scene, ScenePrimitive, SemanticNode, SemanticUpdate, UiId,
        UiInteractionState, UiNode, UiNodeKind, UiRect,
    },
    frame::{DirtyRegionSet, InvalidationRequest, InvalidationSet},
};

mod commit;
mod damage;
mod model;
mod reconcile;
mod scene;
mod semantics;
mod storage;

#[cfg(feature = "diagnostics-timing")]
pub(crate) use model::HostCommitTimings;
pub use model::{
    DamageDetail, DamageReason, DamageReport, HostCommit, HostCommitMetrics, HostMutation,
    HostNodeId, HostRuntime, HostUpdateKind, SceneMutation,
};
use model::{HostNode, HostSlot, SceneNode};

#[cfg(feature = "diagnostics-timing")]
fn elapsed_ms(started: Instant) -> f32 {
    started.elapsed().as_secs_f32() * 1_000.0
}

#[cfg(test)]
mod tests;
