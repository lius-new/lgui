use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use super::geometry::normalized_f32_bits;
use super::{
    BackdropBlurStyle, Color, CompositingLayerSpec, CustomPaintStyle, HostTree, IconStyle,
    OverlayStyle, PathStyle, Point, StaticLayerSpec, Stroke, TextStyle, UiId, UiImageSource,
    UiNode, UiNodeKind, UiPath, UiPathCommand, UiRect, UiScale, VisualStyle,
};

mod compiler;
mod damage;
mod phase;
mod primitive;
mod scene;
mod transform;

pub use compiler::{compile_scene, compile_scene_root, scene_root_ids};
pub use damage::compositing_layer_damage;
pub use phase::commands_for_phase;
pub use primitive::{ImageFit, RenderPhase, ScenePrimitive, ScenePrimitiveKind, ScrollRasterSpec};
pub(crate) use scene::{clear_scroll_raster_command_cache, patch_compositing_layer_spec};
pub use scene::{scroll_raster_command_snapshot_exists, Scene};

#[cfg(test)]
mod tests;
