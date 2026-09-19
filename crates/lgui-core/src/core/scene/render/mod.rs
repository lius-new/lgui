use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use super::{
    normalized_f32_bits, BlurStyle, Color, CompositingLayerSpec, CustomPaintStyle,
    HostTree, IconStyle, ImageFit, ImageRequest, OverlayStyle, PathStyle, Point, StaticLayerSpec,
    Stroke, TextStyle, UiId, UiImageSource, UiNode, UiNodeKind, UiPath, UiPathCommand, UiRect,
    UiScale, VisualStyle,
};

mod compiler;
mod damage;
mod memory;
mod phase;
mod primitive;
mod projection;
mod scene;
mod scroll_cache;
mod signature;
mod transform;

pub use compiler::{compile_scene, compile_scene_root, scene_root_ids};
pub use damage::compositing_layer_damage;
pub(crate) use memory::estimate_scene_commands_bytes;
pub use phase::commands_for_phase;
pub use primitive::{RenderPhase, ScenePrimitive, ScenePrimitiveKind, ScrollRasterSpec};
pub(crate) use scene::patch_compositing_layer_spec;
pub use scene::Scene;
pub use scroll_cache::scroll_raster_command_snapshot_exists;
pub(crate) use scroll_cache::set_scroll_raster_command_cache_budget;
#[doc(hidden)]
pub use transform::translate_scene_primitive_for_backend;

#[path = "shadow_test.rs"]
#[cfg(test)]
mod shadow_tests;
#[path = "render_test.rs"]
#[cfg(test)]
mod tests;
