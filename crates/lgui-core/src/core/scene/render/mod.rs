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
    ImageRequest, OverlayStyle, PathStyle, Point, StaticLayerSpec, Stroke, TextStyle, UiId,
    UiImageSource, UiNode, UiNodeKind, UiPath, UiPathCommand, UiRect, UiScale, VisualStyle,
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
#[cfg(any(test, feature = "backend-winit", feature = "backend-win32"))]
pub(crate) use scene::estimate_scene_commands_bytes;
pub(crate) use scene::patch_compositing_layer_spec;
pub(crate) use scene::{
    scroll_raster_command_cache_usage, set_scroll_raster_command_cache_budget,
    trim_scroll_raster_command_cache,
};
pub use scene::{scroll_raster_command_snapshot_exists, Scene};
#[cfg(all(
    target_os = "windows",
    any(feature = "renderer-gdi", feature = "renderer-d2d")
))]
#[doc(hidden)]
pub use transform::translate_scene_primitive_for_backend;

#[path = "shadow_test.rs"]
#[cfg(test)]
mod shadow_tests;
#[path = "render_test.rs"]
#[cfg(test)]
mod tests;
