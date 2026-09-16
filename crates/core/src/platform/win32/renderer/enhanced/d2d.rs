// Direct2D renderer backend.
//
// Keep backend caches limited to stable reusable resources (images, icons, overlays, static
// layers). Custom paint can include animation-frame state supplied by the caller, so draw it
// immediately and do not insert it into `bitmap_cache` unless a future painter exposes an
// explicit stable-template contract.
use std::{
    collections::hash_map::DefaultHasher,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    mem::ManuallyDrop,
    time::{Duration, Instant},
};

use windows::{
    core::{Error, Interface, Result, HRESULT, HSTRING},
    Win32::Graphics::{
        Direct2D::{
            Common::{
                D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F,
                D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
                D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
            },
            ID2D1Bitmap1, ID2D1ColorContext, ID2D1DeviceContext, ID2D1LinearGradientBrush,
            ID2D1RadialGradientBrush, ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_ALIASED,
            D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_OPTIONS, D2D1_BITMAP_OPTIONS_NONE,
            D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_BUFFER_PRECISION_8BPC_UNORM,
            D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT, D2D1_COLOR_SPACE_SRGB,
            D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
            D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1,
            D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_QUADRATIC_BEZIER_SEGMENT,
            D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT,
        },
        DirectWrite::{
            IDWriteFactory, IDWriteTextLayout1, DWRITE_FONT_STRETCH_NORMAL,
            DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE,
            DWRITE_WORD_WRAPPING_NO_WRAP,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
    },
};

use super::{blur::with_backdrop_blur_bgra, image};
use lgui::core::{
    compositing_layer_damage, Color, CompositingLayerBackground, IconStyle, ImageFit,
    LayerTransform, OverlayStyle, PathStyle, RasterCachePolicy, Scene, ScenePrimitive,
    StaticLayerBackground, StaticLayerSource, StaticLayerSpec, Stroke, TextAlign, UiId,
    UiImageSource, UiPath, UiPathCommand, UiRect, VisualStyle,
};
use lgui::platform::win32::render_trace::{self as trace, TraceCategory};
use lgui::platform::win32::{apply_dwrite_font_fallback, ui_font_family};

mod cache;
mod collection;
mod drawing;
mod effects;
mod renderer;
mod resources;

pub use cache::*;
use collection::*;
use drawing::*;
use effects::*;
use resources::*;

#[path = "d2d/d2d_test.rs"]
#[cfg(test)]
mod tests;
