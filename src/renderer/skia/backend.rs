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
        PhysicalRect, Scene, ScenePrimitive, StaticLayerBackground, StaticLayerCachePolicy,
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

include!("backend/support.rs");
include!("backend/text.rs");
include!("backend/cache.rs");
include!("backend/software.rs");
include!("backend/painter.rs");
include!("backend/primitives.rs");
include!("backend/tests.rs");
