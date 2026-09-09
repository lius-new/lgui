use std::{
    cell::RefCell,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    application::GraphicsPreference,
    assets::render_resources,
    core::{
        BackdropBlurStyle, Color, CompositingLayerBackground, ImageFit, LayerTransform, PathStyle,
        PhysicalRect, RasterCachePolicy, Scene, ScenePrimitive, StaticLayerBackground,
        StaticLayerSource, Stroke, TextAlign, TextStyle, UiImageSource, UiPath, UiPathCommand,
        UiRect, VisualStyle,
    },
    renderer::{FrameInfo, MemoryPressure},
};
use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, RectHeightStyle, RectWidthStyle,
    TextAlign as SkTextAlign, TextDirection as SkTextDirection, TextStyle as SkTextStyle,
    TypefaceFontProvider,
};
use skia_safe::{
    surfaces, AlphaType, BlendMode, Canvas, Color as SkColor, Color4f, ColorType, Data, FontMgr,
    FontStyle, Image, ImageInfo, Paint, PaintStyle, Path, PathBuilder, RRect, Rect,
    SamplingOptions, Surface, TileMode,
};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

mod cache;
mod painter;
mod primitives;
mod software;
mod support;
mod text;

pub(crate) use cache::{SkiaCache, SkiaCacheStats};
#[cfg(any(
    feature = "renderer-skia-gl",
    feature = "renderer-skia-vulkan",
    feature = "renderer-skia-metal"
))]
pub(crate) use software::paint_scene_damage;
pub(crate) use software::SkiaSoftwareSurface;
pub use support::probe_skia_support;
#[cfg(any(
    feature = "renderer-skia-gl",
    feature = "renderer-skia-vulkan",
    feature = "renderer-skia-metal"
))]
pub(crate) use support::{cpu_cache_budget, gpu_cache_budget, with_gpu_cache_usage};
pub(crate) use text::skia_text_system_handle;

use painter::*;
use primitives::*;
use text::*;

#[path = "backend/backend_test.rs"]
#[cfg(test)]
mod tests;
