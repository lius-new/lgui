// GDI renderer backend.
//
// This file should only execute render commands with backend primitives. It must not encode
// caller-specific animation semantics or introduce long-lived caches for custom paint output:
// custom paint may represent a single animation frame.
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{POINT, RECT, SIZE},
        Graphics::{
            Gdi::{
                AlphaBlend, BitBlt, CombineRgn, CreateCompatibleDC, CreateDIBSection, CreateFontW,
                CreatePen, CreatePolygonRgn, CreateRectRgn, CreateSolidBrush, DeleteDC,
                DeleteObject, DrawTextW, Ellipse, FillRect, GdiFlush, GetGlyphIndicesW,
                GetTextExtentPoint32W, LineTo, MoveToEx, RestoreDC, RoundRect, SaveDC,
                SelectClipRgn, SelectObject, SetBkMode, SetTextCharacterExtra, SetTextColor,
                AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
                CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
                DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE,
                GGI_MARK_NONEXISTING_GLYPHS, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ,
                OUT_TT_ONLY_PRECIS, PS_SOLID, RGN_AND, SRCCOPY, TRANSPARENT, WINDING,
            },
            GdiPlus::{
                ColorAdjustTypeBitmap, ColorMatrix, ColorMatrixFlagsDefault, FillModeAlternate,
                GdipAddPathArcI, GdipAddPathBezierI, GdipAddPathLineI, GdipClosePathFigure,
                GdipCreateBitmapFromScan0, GdipCreateFromHDC, GdipCreateImageAttributes,
                GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush,
                GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDisposeImage,
                GdipDisposeImageAttributes, GdipDrawEllipseI, GdipDrawImagePointsRect,
                GdipDrawPath, GdipFillEllipseI, GdipFillPath, GdipSetImageAttributesColorMatrix,
                GdipSetInterpolationMode, GdipSetPixelOffsetMode, GdipSetSmoothingMode, GpBitmap,
                GpBrush, GpGraphics, GpImageAttributes, GpPath, GpPen,
                InterpolationModeHighQualityBicubic, Ok as GpOk, PixelOffsetModeHalf, PointF,
                SmoothingModeAntiAlias, UnitPixel,
            },
        },
    },
};

use std::{
    cell::{Cell, RefCell},
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

use lgui::platform::win32::render_trace as trace;

use super::{
    blur::with_backdrop_blur_bgra,
    image,
    static_layer::{self, StaticLayerDrawBackend},
};
use lgui::core::{
    compositing_layer_damage, Color, CompositingLayerBackground, CompositingLayerSpec,
    LayerTransform, OverlayStyle, PathStyle, PhysicalRect, Point, RadialGradientLayer, Scene,
    ScenePrimitive, Stroke, TextAlign, TextStyle, UiId, UiPath, UiPathCommand, UiRect,
    VerticalGradientLayer, VisualStyle,
};
use lgui::platform::win32::{draw_svg_icon, ui_font_family_at, ui_font_family_count};
use lgui::renderer::ClipRegion;

mod cache;
mod compositing;
mod primitives;
mod renderer;
mod text;

pub(crate) use cache::{
    gdi_renderer_cache_usage, set_gdi_renderer_cache_budget, trim_gdi_renderer_caches,
};
pub use cache::{
    release_gdi_compositing_layer_scope, reset_gdi_frame_blit_metrics, take_gdi_frame_blit_metrics,
    GdiFrameBlitMetrics, GdiFrameBlitSourceMetrics,
};
pub use renderer::GdiRenderer;

use cache::*;
use compositing::*;
use primitives::*;
use text::*;

#[path = "gdi_renderer/gdi_renderer_test.rs"]
#[cfg(test)]
mod tests;
