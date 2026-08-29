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

use super::{
    blur::with_backdrop_blur_bgra, custom_paint::custom_paint_bgra, image,
    static_layer_raster_cache,
};
use lgui::core::{
    compositing_layer_damage, Color, CompositingLayerBackground, CustomPaintStyle, IconStyle,
    ImageFit, LayerTransform, OverlayStyle, PathStyle, Scene, ScenePrimitive,
    StaticLayerBackground, StaticLayerCachePolicy, StaticLayerSource, StaticLayerSpec, Stroke,
    TextAlign, UiId, UiImageSource, UiPath, UiPathCommand, UiRect, VisualStyle,
};
use lgui::platform::win32::render_trace::{self as trace, TraceCategory};
use lgui::platform::win32::{apply_dwrite_font_fallback, ui_font_family};

pub struct D2dRenderer {
    context: ID2D1DeviceContext,
    dwrite_factory: IDWriteFactory,
    scene_bitmap: ID2D1Bitmap1,
    bitmap_cache: D2dBitmapCache,
    overlay_brush_cache: HashMap<D2dOverlayBrushCacheKey, D2dOverlayBrushSet>,
    frame_bitmap_cache: HashMap<D2dBitmapCacheKey, ID2D1Bitmap1>,
    compositing_layers: HashMap<UiId, D2dCompositingLayer>,
}

const D2D_BITMAP_CACHE_MIN_BUDGET_BYTES: usize = 32 * 1024 * 1024;
const D2D_BITMAP_CACHE_VIEWPORT_MULTIPLIER: usize = 4;

fn d2d_bitmap_cache_budget(width: i32, height: i32) -> usize {
    (width.max(1) as usize)
        .saturating_mul(height.max(1) as usize)
        .saturating_mul(4)
        .saturating_mul(D2D_BITMAP_CACHE_VIEWPORT_MULTIPLIER)
        .max(D2D_BITMAP_CACHE_MIN_BUDGET_BYTES)
}

struct D2dBitmapCacheEntry {
    bitmap: ID2D1Bitmap1,
    bytes: usize,
    last_used: u64,
}

struct D2dBitmapCache {
    entries: HashMap<D2dBitmapCacheKey, D2dBitmapCacheEntry>,
    bytes: usize,
    tick: u64,
    budget_bytes: usize,
}

impl D2dBitmapCache {
    fn new(budget_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            tick: 0,
            budget_bytes: budget_bytes.max(1),
        }
    }

    fn retain_live(&mut self, live: &HashSet<D2dBitmapCacheKey>) {
        let mut removed_bytes = 0usize;
        self.entries.retain(|key, entry| {
            let retain = live.contains(key);
            if !retain {
                removed_bytes = removed_bytes.saturating_add(entry.bytes);
            }
            retain
        });
        self.bytes = self.bytes.saturating_sub(removed_bytes);
    }

    fn get(&mut self, key: &D2dBitmapCacheKey) -> Option<ID2D1Bitmap1> {
        self.tick = self.tick.saturating_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.tick;
        Some(entry.bitmap.clone())
    }

    fn insert(&mut self, key: D2dBitmapCacheKey, bitmap: ID2D1Bitmap1) {
        self.tick = self.tick.saturating_add(1);
        let bytes = key.estimated_bytes();
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            D2dBitmapCacheEntry {
                bitmap,
                bytes,
                last_used: self.tick,
            },
        );
    }

    fn evict_to_budget(&mut self) {
        let evictions = bitmap_cache_eviction_plan(
            self.entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry.last_used, entry.bytes)),
            self.bytes,
            self.budget_bytes,
        );
        for key in evictions {
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

fn bitmap_cache_eviction_plan(
    entries: impl IntoIterator<Item = (D2dBitmapCacheKey, u64, usize)>,
    mut bytes: usize,
    budget_bytes: usize,
) -> Vec<D2dBitmapCacheKey> {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_by_key(|(_, last_used, _)| *last_used);
    let mut evictions = Vec::new();
    let mut remaining = entries.len();
    for (key, _, entry_bytes) in entries {
        if bytes <= budget_bytes || remaining <= 1 {
            break;
        }
        bytes = bytes.saturating_sub(entry_bytes);
        remaining -= 1;
        evictions.push(key);
    }
    evictions
}

struct D2dCompositingLayer {
    content_signature: Option<u64>,
    background: CompositingLayerBackground,
    width: i32,
    height: i32,
    bitmap: ID2D1Bitmap1,
    commands: Vec<ScenePrimitive>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct D2dOverlayBrushCacheKey {
    rect: UiRect,
    style_signature: u64,
}

struct D2dOverlayBrushSet {
    linear: Vec<ID2D1LinearGradientBrush>,
    radial: Vec<ID2D1RadialGradientBrush>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum D2dBitmapCacheKey {
    Image {
        source: UiImageSource,
        fit: ImageFit,
        width: i32,
        height: i32,
    },
    Icon {
        key: &'static str,
        color: u32,
        alpha: u8,
        width: i32,
        height: i32,
    },
    BackdropBlur {
        signature: u64,
        width: i32,
        height: i32,
    },
    StaticLayer {
        raster_key: String,
        id: UiId,
        spec_signature: u64,
        child_signature: u64,
        width: i32,
        height: i32,
    },
}

impl D2dBitmapCacheKey {
    fn estimated_bytes(&self) -> usize {
        let (width, height) = match self {
            Self::Image { width, height, .. }
            | Self::Icon { width, height, .. }
            | Self::BackdropBlur { width, height, .. }
            | Self::StaticLayer { width, height, .. } => (*width, *height),
        };
        (width.max(1) as usize)
            .saturating_mul(height.max(1) as usize)
            .saturating_mul(4)
    }
}

impl D2dRenderer {
    pub fn new(
        context: ID2D1DeviceContext,
        dwrite_factory: IDWriteFactory,
        width: i32,
        height: i32,
    ) -> Result<Self> {
        let scene_bitmap = create_scene_bitmap(&context, width, height)?;
        unsafe {
            context.SetTarget(&scene_bitmap);
        }
        Ok(Self {
            context,
            dwrite_factory,
            scene_bitmap,
            bitmap_cache: D2dBitmapCache::new(d2d_bitmap_cache_budget(width, height)),
            overlay_brush_cache: HashMap::new(),
            frame_bitmap_cache: HashMap::new(),
            compositing_layers: HashMap::new(),
        })
    }

    pub fn draw_scene_full(&mut self, list: &Scene) -> Result<()> {
        self.begin_frame(list);
        self.retain_compositing_layers(list);
        self.ensure_static_layer_cache(list, None)?;
        unsafe {
            self.context.SetTarget(&self.scene_bitmap);
            self.context.BeginDraw();
            self.context.Clear(Some(&transparent()));
            draw_scene_d2d(self, list, None)?;
            self.context.EndDraw(None, None)?;
        }
        self.bitmap_cache.evict_to_budget();
        Ok(())
    }

    pub fn draw_scene_dirty(&mut self, list: &Scene, rects: &[UiRect]) -> Result<()> {
        self.begin_frame(list);
        self.retain_compositing_layers(list);
        for rect in rects {
            self.ensure_static_layer_cache(list, Some(*rect))?;
        }
        unsafe {
            self.context.SetTarget(&self.scene_bitmap);
            self.context.BeginDraw();
            for rect in rects {
                let clip = d2d_rect(*rect);
                self.context
                    .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
                self.context.Clear(Some(&transparent()));
                draw_scene_d2d(self, list, Some(*rect))?;
                self.context.PopAxisAlignedClip();
            }
            self.context.EndDraw(None, None)?;
        }
        self.bitmap_cache.evict_to_budget();
        Ok(())
    }

    fn begin_frame(&mut self, list: &Scene) {
        self.frame_bitmap_cache.clear();
        let live = bitmap_cache_keys(list.commands());
        self.bitmap_cache.retain_live(&live);
        let live_overlays = overlay_brush_cache_keys(list.commands());
        self.overlay_brush_cache
            .retain(|key, _| live_overlays.contains(key));
    }

    pub fn copy_scene_to_target(
        &mut self,
        target_bitmap: &ID2D1Bitmap1,
        rects: Option<&[UiRect]>,
    ) -> Result<()> {
        let copy_start = Instant::now();
        unsafe {
            self.context.SetTarget(target_bitmap);
            self.context.BeginDraw();
            match rects {
                Some(rects) => {
                    for rect in rects {
                        let area = d2d_rect(*rect);
                        self.context
                            .PushAxisAlignedClip(&area, D2D1_ANTIALIAS_MODE_ALIASED);
                        self.context.DrawBitmap(
                            &self.scene_bitmap,
                            Some(&area),
                            1.0,
                            D2D1_INTERPOLATION_MODE_LINEAR,
                            Some(&area),
                            None,
                        );
                        self.context.PopAxisAlignedClip();
                    }
                }
                None => {
                    self.context.DrawBitmap(
                        &self.scene_bitmap,
                        None,
                        1.0,
                        D2D1_INTERPOLATION_MODE_LINEAR,
                        None,
                        None,
                    );
                }
            }
            self.context.EndDraw(None, None)?;
        }
        match rects {
            Some(rects) => {
                trace_d2d_regions("presenter.d2d.copy.dirty", Some(rects));
                trace_duration("presenter.d2d.copy.dirty", copy_start.elapsed());
            }
            None => {
                trace_d2d_regions("presenter.d2d.copy.full", None);
                trace_duration("presenter.d2d.copy.full", copy_start.elapsed());
            }
        }
        Ok(())
    }

    fn ensure_static_layer_cache(&mut self, list: &Scene, clip: Option<UiRect>) -> Result<()> {
        for command in list.commands() {
            self.ensure_static_layer_command(command, clip)?;
        }
        Ok(())
    }

    fn retain_compositing_layers(&mut self, list: &Scene) {
        let mut live = std::collections::HashSet::new();
        collect_compositing_layer_ids(list.commands(), &mut live);
        self.compositing_layers.retain(|id, _| live.contains(id));
    }

    fn ensure_static_layer_command(
        &mut self,
        command: &ScenePrimitive,
        clip: Option<UiRect>,
    ) -> Result<()> {
        if clip
            .and_then(|clip| clip.intersect(command.paint_bounds()))
            .is_none()
            && clip.is_some()
        {
            return Ok(());
        }
        match command {
            ScenePrimitive::CompositingLayer {
                id,
                rect,
                spec,
                commands,
                content_signature,
                ..
            } => {
                let width = rect.width().max(1);
                let height = rect.height().max(1);
                let previous = self.compositing_layers.remove(id);
                let mut layer = match previous {
                    Some(layer)
                        if layer.width == width
                            && layer.height == height
                            && layer.background == spec.background =>
                    {
                        layer
                    }
                    _ => create_compositing_layer(self, width, height, spec.background)?,
                };
                if layer.content_signature != Some(*content_signature) {
                    for command in commands {
                        self.ensure_static_layer_command(command, None)?;
                    }
                    let bounds = UiRect::new(0, 0, width, height);
                    let damage = if layer.commands.is_empty() {
                        vec![bounds]
                    } else {
                        compositing_layer_damage(&layer.commands, commands, bounds)
                    };
                    redraw_compositing_layer(
                        self,
                        &layer.bitmap,
                        spec.background,
                        commands,
                        &damage,
                    )?;
                    layer.content_signature = Some(*content_signature);
                    layer.commands = commands.clone();
                }
                self.compositing_layers.insert(id.clone(), layer);
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                if pure_static_layer_image(spec, commands).is_some() {
                    return Ok(());
                }
                let cache_key = static_layer_cache_key(
                    id,
                    spec,
                    rect.width().max(1),
                    rect.height().max(1),
                    *child_signature,
                );
                if spec.cache_policy == StaticLayerCachePolicy::Disabled {
                    if !self.frame_bitmap_cache.contains_key(&cache_key) {
                        for command in commands {
                            self.ensure_static_layer_command(command, clip)?;
                        }
                        let bitmap = render_static_layer_bitmap(
                            self,
                            *rect,
                            spec,
                            commands,
                            Some(&cache_key),
                        )?;
                        self.frame_bitmap_cache.insert(cache_key, bitmap);
                    }
                } else if self.bitmap_cache.get(&cache_key).is_none() {
                    for command in commands {
                        self.ensure_static_layer_command(command, clip)?;
                    }
                    let bitmap =
                        render_static_layer_bitmap(self, *rect, spec, commands, Some(&cache_key))?;
                    self.bitmap_cache.insert(cache_key, bitmap);
                }
            }
            ScenePrimitive::Clip { rect, commands, .. } => {
                let nested_clip = clip.and_then(|clip| clip.intersect(*rect)).or(Some(*rect));
                for command in commands {
                    self.ensure_static_layer_command(command, nested_clip)?;
                }
            }
            ScenePrimitive::ScrollRaster {
                viewport,
                spec,
                commands,
                ..
            } => {
                let nested_clip = clip
                    .and_then(|clip| clip.intersect(*viewport))
                    .or(Some(*viewport));
                let translated = commands
                    .iter()
                    .map(|command| translate_command(command, 0, -spec.scroll_y))
                    .collect::<Vec<_>>();
                for command in &translated {
                    self.ensure_static_layer_command(command, nested_clip)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn bitmap_cache_keys(commands: &[ScenePrimitive]) -> HashSet<D2dBitmapCacheKey> {
    let mut keys = HashSet::new();
    collect_bitmap_cache_keys(commands, &mut keys);
    keys
}

fn overlay_brush_cache_keys(commands: &[ScenePrimitive]) -> HashSet<D2dOverlayBrushCacheKey> {
    let mut keys = HashSet::new();
    collect_overlay_brush_cache_keys(commands, &mut keys);
    keys
}

fn collect_compositing_layer_ids(
    commands: &[ScenePrimitive],
    ids: &mut std::collections::HashSet<UiId>,
) {
    for command in commands {
        match command {
            ScenePrimitive::CompositingLayer { id, commands, .. } => {
                ids.insert(id.clone());
                collect_compositing_layer_ids(commands, ids);
            }
            ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_compositing_layer_ids(commands, ids);
            }
            _ => {}
        }
    }
}

fn collect_bitmap_cache_keys(commands: &[ScenePrimitive], keys: &mut HashSet<D2dBitmapCacheKey>) {
    for command in commands {
        match command {
            ScenePrimitive::Image {
                rect, source, fit, ..
            } => {
                keys.insert(image_cache_key(*rect, source, *fit));
            }
            ScenePrimitive::Icon {
                rect, key, style, ..
            } => {
                keys.insert(icon_cache_key(*rect, key, *style));
            }
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                keys.insert(backdrop_blur_cache_key(*rect, *style));
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                if let Some((source, fit)) = pure_static_layer_image(spec, commands) {
                    keys.insert(image_cache_key(
                        UiRect::new(0, 0, rect.width().max(1), rect.height().max(1)),
                        &UiImageSource::Static(source),
                        fit,
                    ));
                } else if spec.cache_policy == StaticLayerCachePolicy::Disabled {
                    collect_bitmap_cache_keys(commands, keys);
                } else {
                    keys.insert(static_layer_cache_key(
                        id,
                        spec,
                        rect.width().max(1),
                        rect.height().max(1),
                        *child_signature,
                    ));
                }
            }
            ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_bitmap_cache_keys(commands, keys);
            }
            ScenePrimitive::CompositingLayer { .. }
            | ScenePrimitive::Rect { .. }
            | ScenePrimitive::Ellipse { .. }
            | ScenePrimitive::Text { .. }
            | ScenePrimitive::Custom { .. }
            | ScenePrimitive::Line { .. }
            | ScenePrimitive::Path { .. }
            | ScenePrimitive::Glow { .. }
            | ScenePrimitive::BackdropBlurPath { .. }
            | ScenePrimitive::Overlay { .. } => {}
        }
    }
}

fn collect_overlay_brush_cache_keys(
    commands: &[ScenePrimitive],
    keys: &mut HashSet<D2dOverlayBrushCacheKey>,
) {
    for command in commands {
        match command {
            ScenePrimitive::Overlay { rect, style, .. } => {
                keys.insert(overlay_brush_cache_key(*rect, style));
            }
            ScenePrimitive::StaticLayer { spec, commands, .. }
                if spec.cache_policy == StaticLayerCachePolicy::Disabled =>
            {
                collect_overlay_brush_cache_keys(commands, keys);
            }
            ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_overlay_brush_cache_keys(commands, keys);
            }
            _ => {}
        }
    }
}

fn pure_static_layer_image(
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
) -> Option<(&'static str, ImageFit)> {
    if !commands.is_empty() || spec.background != StaticLayerBackground::Transparent {
        return None;
    }
    match spec.source {
        StaticLayerSource::BakedAsset { key, fit } => Some((key, fit)),
        StaticLayerSource::Hybrid {
            baked_base: Some(key),
            fit,
        } => Some((key, fit)),
        StaticLayerSource::RuntimeGenerated
        | StaticLayerSource::Hybrid {
            baked_base: None, ..
        } => None,
    }
}

fn draw_scene_d2d(resources: &mut D2dRenderer, list: &Scene, clip: Option<UiRect>) -> Result<()> {
    draw_commands_d2d(resources, list.commands(), clip)
}

fn draw_commands_d2d(
    resources: &mut D2dRenderer,
    commands: &[ScenePrimitive],
    clip: Option<UiRect>,
) -> Result<()> {
    for command in commands {
        if clip
            .and_then(|clip| clip.intersect(command.paint_bounds()))
            .is_none()
            && clip.is_some()
        {
            continue;
        }
        draw_command_d2d(resources, command)?;
    }
    Ok(())
}

fn draw_command_d2d(resources: &mut D2dRenderer, command: &ScenePrimitive) -> Result<()> {
    match command {
        ScenePrimitive::Rect { rect, style, .. } => draw_rect(&resources.context, *rect, *style),
        ScenePrimitive::Ellipse { rect, style, .. } => {
            draw_ellipse(&resources.context, *rect, *style)
        }
        ScenePrimitive::Text {
            rect, text, style, ..
        } => draw_text(
            &resources.context,
            &resources.dwrite_factory,
            *rect,
            text,
            *style,
        ),
        ScenePrimitive::Line {
            start, end, stroke, ..
        } => draw_line(&resources.context, *start, *end, *stroke),
        ScenePrimitive::Path { path, style, .. } => draw_path(&resources.context, path, *style),
        ScenePrimitive::Image {
            rect, source, fit, ..
        } => draw_image(resources, *rect, source, *fit),
        ScenePrimitive::Icon {
            rect, key, style, ..
        } => draw_icon(resources, *rect, key, *style),
        ScenePrimitive::Overlay { rect, style, .. } => draw_overlay(resources, *rect, style),
        ScenePrimitive::CompositingLayer { id, rect, spec, .. } => {
            let Some(layer) = resources.compositing_layers.get(id) else {
                return Ok(());
            };
            draw_compositing_layer_bitmap(
                &resources.context,
                *rect,
                &layer.bitmap,
                spec.opacity_f32(),
                spec.transform,
            );
            Ok(())
        }
        ScenePrimitive::BackdropBlur { rect, style, .. } => {
            draw_backdrop_blur(resources, *rect, *style)
        }
        ScenePrimitive::BackdropBlurPath {
            rect, path, style, ..
        } => draw_backdrop_blur_path(resources, *rect, path, *style),
        ScenePrimitive::Custom {
            rect, key, style, ..
        } => {
            if let Some(style) = style {
                draw_custom_effect(resources, *rect, key, *style)
            } else {
                Ok(())
            }
        }
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            ..
        } => draw_static_layer(resources, id, *rect, spec, commands, *child_signature),
        ScenePrimitive::ScrollRaster {
            viewport,
            spec,
            commands,
            ..
        } => {
            let area = d2d_rect(*viewport);
            unsafe {
                resources
                    .context
                    .PushAxisAlignedClip(&area, D2D1_ANTIALIAS_MODE_ALIASED);
            }
            let translated = commands
                .iter()
                .map(|command| translate_command(command, 0, -spec.scroll_y))
                .collect::<Vec<_>>();
            let result = draw_commands_d2d(resources, &translated, Some(*viewport));
            unsafe {
                resources.context.PopAxisAlignedClip();
            }
            result
        }
        ScenePrimitive::Clip { rect, commands, .. } => {
            let area = d2d_rect(*rect);
            unsafe {
                resources
                    .context
                    .PushAxisAlignedClip(&area, D2D1_ANTIALIAS_MODE_ALIASED);
            }
            let result = draw_commands_d2d(resources, commands, Some(*rect));
            unsafe {
                resources.context.PopAxisAlignedClip();
            }
            result
        }
        ScenePrimitive::ClipPath {
            rect,
            path,
            commands,
            ..
        } => {
            let area = d2d_rect(*rect);
            let geometry = create_path_geometry(&resources.context, path)?;
            let parameters = D2D1_LAYER_PARAMETERS1 {
                contentBounds: area,
                geometricMask: ManuallyDrop::new(Some(geometry.into())),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: windows_numerics::Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            unsafe {
                resources.context.PushLayer(&parameters, None);
            }
            let result = draw_commands_d2d(resources, commands, Some(*rect));
            unsafe {
                resources.context.PopLayer();
            }
            result
        }
        ScenePrimitive::Glow {
            rect, color, alpha, ..
        } => {
            let style = OverlayStyle::new().radial(lgui::core::RadialGradientLayer::new(
                *color,
                *alpha as f32 / 255.0,
                0.5,
                0.5,
                0.5,
            ));
            draw_overlay(resources, *rect, &style)
        }
    }
}

fn draw_rect(context: &ID2D1DeviceContext, rect: UiRect, style: VisualStyle) -> Result<()> {
    unsafe {
        if let Some(fill) = style.fill {
            let brush = solid_brush(context, fill, style.fill_alpha)?;
            if style.radius > 0 {
                let rounded = rounded_rect(rect, style.radius);
                context.FillRoundedRectangle(&rounded, &brush);
            } else {
                let rect = d2d_rect(rect);
                context.FillRectangle(&rect, &brush);
            }
        }
        if let Some(stroke) = style.stroke {
            let brush = solid_brush(context, stroke.color, stroke.alpha)?;
            if style.radius > 0 {
                let rounded = rounded_rect(rect, style.radius);
                context.DrawRoundedRectangle(&rounded, &brush, stroke.width as f32, None);
            } else {
                let rect = d2d_rect(rect);
                context.DrawRectangle(&rect, &brush, stroke.width as f32, None);
            }
        }
    }
    Ok(())
}

fn draw_ellipse(context: &ID2D1DeviceContext, rect: UiRect, style: VisualStyle) -> Result<()> {
    let ellipse = D2D1_ELLIPSE {
        point: windows_numerics::Vector2 {
            X: (rect.left + rect.right) as f32 / 2.0,
            Y: (rect.top + rect.bottom) as f32 / 2.0,
        },
        radiusX: rect.width() as f32 / 2.0,
        radiusY: rect.height() as f32 / 2.0,
    };
    unsafe {
        if let Some(fill) = style.fill {
            let brush = solid_brush(context, fill, style.fill_alpha)?;
            context.FillEllipse(&ellipse, &brush);
        }
        if let Some(stroke) = style.stroke {
            let brush = solid_brush(context, stroke.color, stroke.alpha)?;
            context.DrawEllipse(&ellipse, &brush, stroke.width as f32, None);
        }
    }
    Ok(())
}

fn draw_path(context: &ID2D1DeviceContext, path: &UiPath, style: PathStyle) -> Result<()> {
    if path.commands().is_empty() {
        return Ok(());
    }

    let geometry = create_path_geometry(context, path)?;
    unsafe {
        if let Some(fill) = style.fill {
            let brush = solid_brush(context, fill, style.fill_alpha)?;
            context.FillGeometry(&geometry, &brush, None);
        }
        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0 {
                let brush = solid_brush(context, stroke.color, stroke.alpha)?;
                context.DrawGeometry(&geometry, &brush, stroke.width as f32, None);
            }
        }
    }
    Ok(())
}

fn create_path_geometry(
    context: &ID2D1DeviceContext,
    path: &UiPath,
) -> Result<windows::Win32::Graphics::Direct2D::ID2D1PathGeometry> {
    unsafe {
        let factory = context.GetFactory()?;
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;

        let mut figure_open = false;
        for command in path.commands() {
            match *command {
                UiPathCommand::MoveTo(point) => {
                    if figure_open {
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    }
                    sink.BeginFigure(vector2(point.x, point.y), D2D1_FIGURE_BEGIN_FILLED);
                    figure_open = true;
                }
                UiPathCommand::LineTo(point) => {
                    if figure_open {
                        sink.AddLine(vector2(point.x, point.y));
                    } else {
                        sink.BeginFigure(vector2(point.x, point.y), D2D1_FIGURE_BEGIN_FILLED);
                        figure_open = true;
                    }
                }
                UiPathCommand::QuadraticTo { control, to } => {
                    if figure_open {
                        let segment = D2D1_QUADRATIC_BEZIER_SEGMENT {
                            point1: vector2(control.x, control.y),
                            point2: vector2(to.x, to.y),
                        };
                        sink.AddQuadraticBezier(&segment);
                    } else {
                        sink.BeginFigure(vector2(to.x, to.y), D2D1_FIGURE_BEGIN_FILLED);
                        figure_open = true;
                    }
                }
                UiPathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    if figure_open {
                        let segment = D2D1_BEZIER_SEGMENT {
                            point1: vector2(control1.x, control1.y),
                            point2: vector2(control2.x, control2.y),
                            point3: vector2(to.x, to.y),
                        };
                        sink.AddBezier(&segment);
                    } else {
                        sink.BeginFigure(vector2(to.x, to.y), D2D1_FIGURE_BEGIN_FILLED);
                        figure_open = true;
                    }
                }
                UiPathCommand::Close => {
                    if figure_open {
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        figure_open = false;
                    }
                }
            }
        }
        if figure_open {
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close()?;
        Ok(geometry)
    }
}

fn draw_line(
    context: &ID2D1DeviceContext,
    start: lgui::core::Point,
    end: lgui::core::Point,
    stroke: Stroke,
) -> Result<()> {
    let brush = solid_brush(context, stroke.color, stroke.alpha)?;
    unsafe {
        context.DrawLine(
            windows_numerics::Vector2 {
                X: start.x as f32,
                Y: start.y as f32,
            },
            windows_numerics::Vector2 {
                X: end.x as f32,
                Y: end.y as f32,
            },
            &brush,
            stroke.width as f32,
            None,
        );
    }
    Ok(())
}

fn draw_image(
    resources: &mut D2dRenderer,
    rect: UiRect,
    source: &UiImageSource,
    fit: ImageFit,
) -> Result<()> {
    let Some(bitmap) = image_bitmap(resources, rect, source, fit)? else {
        return Ok(());
    };
    draw_bitmap(&resources.context, rect, &bitmap);
    Ok(())
}

fn image_cache_key(rect: UiRect, source: &UiImageSource, fit: ImageFit) -> D2dBitmapCacheKey {
    D2dBitmapCacheKey::Image {
        source: source.clone(),
        fit,
        width: rect.width().max(1),
        height: rect.height().max(1),
    }
}

fn image_bitmap(
    resources: &mut D2dRenderer,
    rect: UiRect,
    source: &UiImageSource,
    fit: ImageFit,
) -> Result<Option<ID2D1Bitmap1>> {
    // Image cache is for stable asset sources only. Do not route dynamic raster output here.
    let key = image_cache_key(rect, source, fit);
    if let Some(bitmap) = resources.bitmap_cache.get(&key) {
        return Ok(Some(bitmap));
    }
    let Some(image) = image::rasterize_ui_image_bgra(source, rect, fit) else {
        return Ok(None);
    };
    let bitmap = create_bgra_bitmap(
        &resources.context,
        image.width,
        image.height,
        &image.premultiplied_bgra,
    )?;
    resources.bitmap_cache.insert(key, bitmap.clone());
    Ok(Some(bitmap))
}

fn draw_icon(
    resources: &mut D2dRenderer,
    rect: UiRect,
    key: &'static str,
    style: IconStyle,
) -> Result<()> {
    // Icon cache is valid while key/style/size are stable. Highly dynamic icon styling should
    // avoid producing unbounded cache keys.
    let cache_key = icon_cache_key(rect, key, style);
    let bitmap = if let Some(bitmap) = resources.bitmap_cache.get(&cache_key) {
        bitmap
    } else {
        let Some(icon) = lgui::platform::win32::rasterize_svg_icon_bgra(key, rect, style) else {
            return Ok(());
        };
        let bitmap = create_bgra_bitmap(
            &resources.context,
            icon.width,
            icon.height,
            &icon.premultiplied_bgra,
        )?;
        resources.bitmap_cache.insert(cache_key, bitmap.clone());
        bitmap
    };
    draw_bitmap(&resources.context, rect, &bitmap);
    Ok(())
}

fn icon_cache_key(rect: UiRect, key: &'static str, style: IconStyle) -> D2dBitmapCacheKey {
    D2dBitmapCacheKey::Icon {
        key,
        color: style.color.0,
        alpha: style.alpha,
        width: rect.width().max(1),
        height: rect.height().max(1),
    }
}

fn create_compositing_layer(
    resources: &mut D2dRenderer,
    width: i32,
    height: i32,
    background: CompositingLayerBackground,
) -> Result<D2dCompositingLayer> {
    let bitmap = create_scene_bitmap(&resources.context, width, height)?;
    Ok(D2dCompositingLayer {
        content_signature: None,
        background,
        width,
        height,
        bitmap,
        commands: Vec::new(),
    })
}

fn redraw_compositing_layer(
    resources: &mut D2dRenderer,
    bitmap: &ID2D1Bitmap1,
    background: CompositingLayerBackground,
    commands: &[ScenePrimitive],
    damage: &[UiRect],
) -> Result<()> {
    if damage.is_empty() {
        return Ok(());
    }
    let clear = match background {
        CompositingLayerBackground::Opaque => opaque_black(),
        CompositingLayerBackground::Transparent => transparent(),
    };
    unsafe {
        resources.context.SetTarget(bitmap);
        resources.context.BeginDraw();
    }
    let mut draw_result = Ok(());
    for rect in damage {
        let clip = d2d_rect(*rect);
        unsafe {
            resources
                .context
                .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
            resources.context.Clear(Some(&clear));
        }
        if let Err(error) = draw_commands_d2d(resources, commands, Some(*rect)) {
            draw_result = Err(error);
        }
        unsafe {
            resources.context.PopAxisAlignedClip();
        }
        if draw_result.is_err() {
            break;
        }
    }
    let end_result = unsafe { resources.context.EndDraw(None, None) };
    unsafe {
        resources.context.SetTarget(&resources.scene_bitmap);
    }
    draw_result?;
    end_result?;
    Ok(())
}

fn draw_static_layer(
    resources: &mut D2dRenderer,
    id: &UiId,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    child_signature: u64,
) -> Result<()> {
    let draw_rect = rect.translate(spec.offset_x, spec.offset_y);
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    if let Some((source, fit)) = pure_static_layer_image(spec, commands) {
        let local_rect = UiRect::new(0, 0, width, height);
        if let Some(bitmap) =
            image_bitmap(resources, local_rect, &UiImageSource::Static(source), fit)?
        {
            draw_bitmap_opacity(&resources.context, draw_rect, &bitmap, spec.opacity_f32());
        }
        return Ok(());
    }
    let cache_key = static_layer_cache_key(id, spec, width, height, child_signature);
    if spec.cache_policy == StaticLayerCachePolicy::Disabled {
        let bitmap = if let Some(bitmap) = resources.frame_bitmap_cache.get(&cache_key) {
            bitmap.clone()
        } else {
            let bitmap = render_static_layer_bitmap(resources, rect, spec, commands, None)?;
            resources
                .frame_bitmap_cache
                .insert(cache_key, bitmap.clone());
            unsafe {
                resources.context.SetTarget(&resources.scene_bitmap);
            }
            bitmap
        };
        draw_bitmap_opacity(&resources.context, draw_rect, &bitmap, spec.opacity_f32());
        return Ok(());
    }
    let bitmap = if let Some(bitmap) = resources.bitmap_cache.get(&cache_key) {
        bitmap
    } else {
        let bitmap = render_static_layer_bitmap(resources, rect, spec, commands, Some(&cache_key))?;
        resources.bitmap_cache.insert(cache_key, bitmap.clone());
        unsafe {
            resources.context.SetTarget(&resources.scene_bitmap);
        }
        bitmap
    };
    draw_bitmap_opacity(&resources.context, draw_rect, &bitmap, spec.opacity_f32());
    Ok(())
}

fn render_static_layer_bitmap(
    resources: &mut D2dRenderer,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    cache_key: Option<&D2dBitmapCacheKey>,
) -> Result<ID2D1Bitmap1> {
    let start = Instant::now();
    if spec.cache_policy == StaticLayerCachePolicy::MemoryAndDisk {
        if let Some(D2dBitmapCacheKey::StaticLayer { raster_key, .. }) = cache_key {
            if let Some(cached) = static_layer_raster_cache::load(raster_key) {
                trace_duration("d2d.static_layer.raster_hit", start.elapsed());
                return create_bgra_bitmap(
                    &resources.context,
                    cached.width,
                    cached.height,
                    &cached.premultiplied_bgra,
                );
            }
        }
    }

    let width = rect.width().max(1);
    let height = rect.height().max(1);
    let bitmap = create_scene_bitmap(&resources.context, width, height)?;
    unsafe {
        resources.context.SetTarget(&bitmap);
        resources.context.BeginDraw();
        resources
            .context
            .Clear(Some(&static_layer_clear_color(spec)));
    }
    let local_rect = UiRect::new(0, 0, width, height);
    match &spec.source {
        StaticLayerSource::BakedAsset { key, fit } => {
            draw_image(resources, local_rect, &UiImageSource::Static(key), *fit)?;
        }
        StaticLayerSource::RuntimeGenerated => {}
        StaticLayerSource::Hybrid { baked_base, fit } => {
            if let Some(key) = baked_base {
                draw_image(resources, local_rect, &UiImageSource::Static(key), *fit)?;
            }
        }
    }

    for command in commands {
        let local = translate_command(command, -rect.left, -rect.top);
        draw_command_d2d(resources, &local)?;
    }
    unsafe {
        resources.context.EndDraw(None, None)?;
    }

    trace_duration("d2d.static_layer.generate", start.elapsed());
    Ok(bitmap)
}

fn translate_command(command: &ScenePrimitive, dx: i32, dy: i32) -> ScenePrimitive {
    let translate_rect = |rect: UiRect| {
        UiRect::new(
            rect.left + dx,
            rect.top + dy,
            rect.right + dx,
            rect.bottom + dy,
        )
    };
    let translate_point =
        |point: lgui::core::Point| lgui::core::Point::new(point.x + dx, point.y + dy);
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Rect {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Ellipse {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => ScenePrimitive::Text {
            id: id.clone(),
            rect: translate_rect(*rect),
            text: text.clone(),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Custom {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => ScenePrimitive::Line {
            id: id.clone(),
            start: translate_point(*start),
            end: translate_point(*end),
            stroke: *stroke,
            phase: *phase,
        },
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::Path {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Image {
            id,
            rect,
            source,
            fit,
            phase,
        } => ScenePrimitive::Image {
            id: id.clone(),
            rect: translate_rect(*rect),
            source: source.clone(),
            fit: *fit,
            phase: *phase,
        },
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Icon {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => ScenePrimitive::Glow {
            id: id.clone(),
            rect: translate_rect(*rect),
            color: *color,
            alpha: *alpha,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::BackdropBlur {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::BackdropBlurPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Overlay {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: style.clone(),
            phase: *phase,
        },
        ScenePrimitive::CompositingLayer {
            id,
            rect,
            spec,
            commands,
            content_signature,
            phase,
        } => ScenePrimitive::CompositingLayer {
            id: id.clone(),
            rect: translate_rect(*rect),
            spec: *spec,
            commands: commands.clone(),
            content_signature: *content_signature,
            phase: *phase,
        },
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::StaticLayer {
            id: id.clone(),
            rect: translate_rect(*rect),
            spec: spec.clone(),
            commands: commands.clone(),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ScrollRaster {
            id: id.clone(),
            viewport: translate_rect(*viewport),
            spec: spec.clone(),
            commands: commands
                .iter()
                .map(|command| translate_command(command, dx, dy))
                .collect(),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::Clip {
            id,
            rect,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::Clip {
            id: id.clone(),
            rect: translate_rect(*rect),
            commands: commands
                .iter()
                .map(|command| translate_command(command, dx, dy))
                .collect(),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ClipPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            commands: commands
                .iter()
                .map(|command| translate_command(command, dx, dy))
                .collect(),
            child_signature: *child_signature,
            phase: *phase,
        },
    }
}

fn translate_path(path: &UiPath, dx: i32, dy: i32) -> UiPath {
    use lgui::core::Point;

    let translate = |point: Point| Point::new(point.x + dx, point.y + dy);
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(translate(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(translate(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: translate(control),
            to: translate(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: translate(control1),
            control2: translate(control2),
            to: translate(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}

fn draw_custom_effect(
    resources: &mut D2dRenderer,
    rect: UiRect,
    key_name: &'static str,
    style: CustomPaintStyle,
) -> Result<()> {
    let width = rect.width().max(1);
    let height = rect.height().max(1);
    // Custom paint can be animation-frame dependent. Do not store these bitmaps in the
    // long-lived D2D bitmap cache; cache only stable assets/layers with reusable keys.
    let raster_start = Instant::now();
    let Some(pixels) = custom_paint_bgra(key_name, width, height, style) else {
        return Ok(());
    };
    trace_custom_duration("d2d.draw_custom.raster", key_name, raster_start.elapsed());
    let bitmap_start = Instant::now();
    let bitmap = create_bgra_bitmap(&resources.context, width, height, &pixels)?;
    trace_custom_duration("d2d.draw_custom.bitmap", key_name, bitmap_start.elapsed());
    let draw_start = Instant::now();
    draw_bitmap(&resources.context, rect, &bitmap);
    trace_custom_duration("d2d.draw_custom.draw", key_name, draw_start.elapsed());
    Ok(())
}

fn draw_overlay(resources: &mut D2dRenderer, rect: UiRect, style: &OverlayStyle) -> Result<()> {
    let start = Instant::now();
    let area = d2d_rect(rect);
    let key = ensure_overlay_brush_set(resources, rect, style)?;
    let brushes = resources
        .overlay_brush_cache
        .get(&key)
        .expect("overlay brush set must exist after ensure");
    unsafe {
        for brush in &brushes.linear {
            resources.context.FillRectangle(&area, brush);
        }
        for brush in &brushes.radial {
            resources.context.FillRectangle(&area, brush);
        }
    }
    trace_duration("d2d.draw_overlay", start.elapsed());
    Ok(())
}

fn ensure_overlay_brush_set(
    resources: &mut D2dRenderer,
    rect: UiRect,
    style: &OverlayStyle,
) -> Result<D2dOverlayBrushCacheKey> {
    let key = overlay_brush_cache_key(rect, style);
    if resources.overlay_brush_cache.contains_key(&key) {
        return Ok(key);
    }

    let width = rect.width().max(1) as f32;
    let height = rect.height().max(1) as f32;
    let mut linear = Vec::with_capacity(style.vertical_layers.len());
    let mut radial = Vec::with_capacity(style.radial_layers.len());
    unsafe {
        for layer in &style.vertical_layers {
            let stops = [
                D2D1_GRADIENT_STOP {
                    position: 0.0,
                    color: d2d_color_alpha(layer.color, layer.alpha_top),
                },
                D2D1_GRADIENT_STOP {
                    position: 1.0,
                    color: d2d_color_alpha(layer.color, layer.alpha_bottom),
                },
            ];
            let collection = resources.context.CreateGradientStopCollection(
                &stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                D2D1_EXTEND_MODE_CLAMP,
                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
            )?;
            let properties = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: windows_numerics::Vector2 {
                    X: rect.left as f32,
                    Y: rect.top as f32,
                },
                endPoint: windows_numerics::Vector2 {
                    X: rect.left as f32,
                    Y: rect.top as f32 + height,
                },
            };
            linear.push(resources.context.CreateLinearGradientBrush(
                &properties,
                None,
                &collection,
            )?);
        }
        for layer in &style.radial_layers {
            let stops = radial_gradient_stops(*layer);
            let collection = resources.context.CreateGradientStopCollection(
                &stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                D2D1_EXTEND_MODE_CLAMP,
                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
            )?;
            let radius = (width.min(height) * layer.radius).max(1.0);
            let properties = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: windows_numerics::Vector2 {
                    X: rect.left as f32 + width * layer.center_x,
                    Y: rect.top as f32 + height * layer.center_y,
                },
                gradientOriginOffset: windows_numerics::Vector2 { X: 0.0, Y: 0.0 },
                radiusX: radius,
                radiusY: radius,
            };
            radial.push(resources.context.CreateRadialGradientBrush(
                &properties,
                None,
                &collection,
            )?);
        }
    }
    let brushes = D2dOverlayBrushSet { linear, radial };
    resources.overlay_brush_cache.insert(key.clone(), brushes);
    Ok(key)
}

fn overlay_brush_cache_key(rect: UiRect, style: &OverlayStyle) -> D2dOverlayBrushCacheKey {
    D2dOverlayBrushCacheKey {
        rect,
        style_signature: overlay_signature(style),
    }
}

fn overlay_signature(style: &OverlayStyle) -> u64 {
    let mut hasher = DefaultHasher::new();
    style.vertical_layers.len().hash(&mut hasher);
    for layer in &style.vertical_layers {
        layer.color.0.hash(&mut hasher);
        layer.alpha_top.to_bits().hash(&mut hasher);
        layer.alpha_bottom.to_bits().hash(&mut hasher);
    }
    style.radial_layers.len().hash(&mut hasher);
    for layer in &style.radial_layers {
        layer.color.0.hash(&mut hasher);
        layer.alpha.to_bits().hash(&mut hasher);
        layer.center_x.to_bits().hash(&mut hasher);
        layer.center_y.to_bits().hash(&mut hasher);
        layer.radius.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn radial_gradient_stops(layer: lgui::core::RadialGradientLayer) -> [D2D1_GRADIENT_STOP; 5] {
    [0.0_f32, 0.25, 0.5, 0.75, 1.0].map(|position| D2D1_GRADIENT_STOP {
        position,
        color: d2d_color_alpha(layer.color, layer.alpha * (1.0 - position).powi(2)),
    })
}

fn draw_backdrop_blur(
    resources: &mut D2dRenderer,
    rect: UiRect,
    style: lgui::core::BackdropBlurStyle,
) -> Result<()> {
    let Some(result) = with_backdrop_blur_bgra(rect, style, |pixels, width, height, opacity| {
        if opacity <= 0.0 {
            return Ok(());
        }
        let key = backdrop_blur_cache_key(rect, style);
        let bitmap = if let Some(bitmap) = resources.bitmap_cache.get(&key) {
            bitmap
        } else {
            let bitmap = create_bgra_bitmap(&resources.context, width, height, pixels)?;
            resources.bitmap_cache.insert(key, bitmap.clone());
            bitmap
        };
        draw_bitmap_opacity(&resources.context, rect, &bitmap, opacity);
        Ok(())
    }) else {
        return Ok(());
    };
    result
}

fn backdrop_blur_cache_key(
    rect: UiRect,
    style: lgui::core::BackdropBlurStyle,
) -> D2dBitmapCacheKey {
    D2dBitmapCacheKey::BackdropBlur {
        signature: backdrop_blur_signature(rect, style),
        width: rect.width().max(1),
        height: rect.height().max(1),
    }
}

fn draw_backdrop_blur_path(
    resources: &mut D2dRenderer,
    rect: UiRect,
    path: &UiPath,
    style: lgui::core::BackdropBlurStyle,
) -> Result<()> {
    let Some(result) = with_backdrop_blur_bgra(rect, style, |pixels, width, height, opacity| {
        if opacity <= 0.0 {
            return Ok(());
        }
        let mut masked = pixels.to_vec();
        if let Some(points) = polygon_points(path) {
            mask_polygon(&mut masked, width, height, rect, &points);
        }
        let bitmap = create_bgra_bitmap(&resources.context, width, height, &masked)?;
        draw_bitmap_opacity(&resources.context, rect, &bitmap, opacity);
        Ok(())
    }) else {
        return Ok(());
    };
    result
}

fn create_bgra_bitmap(
    context: &ID2D1DeviceContext,
    width: i32,
    height: i32,
    pixels: &[u8],
) -> Result<ID2D1Bitmap1> {
    if width <= 0 || height <= 0 || pixels.len() != (width * height * 4) as usize {
        return Err(Error::from_hresult(HRESULT(0x80070057u32 as i32)));
    }
    unsafe {
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
            colorContext: ManuallyDrop::<Option<ID2D1ColorContext>>::new(None),
        };
        context.CreateBitmap(
            D2D_SIZE_U {
                width: width as u32,
                height: height as u32,
            },
            Some(pixels.as_ptr().cast()),
            (width * 4) as u32,
            &props,
        )
    }
}

fn draw_bitmap(context: &ID2D1DeviceContext, rect: UiRect, bitmap: &ID2D1Bitmap1) {
    draw_bitmap_opacity(context, rect, bitmap, 1.0);
}

fn draw_bitmap_opacity(
    context: &ID2D1DeviceContext,
    rect: UiRect,
    bitmap: &ID2D1Bitmap1,
    opacity: f32,
) {
    unsafe {
        let dest = d2d_rect(rect);
        context.DrawBitmap(
            bitmap,
            Some(&dest),
            opacity.clamp(0.0, 1.0),
            D2D1_INTERPOLATION_MODE_LINEAR,
            None,
            None,
        );
    }
}

fn draw_compositing_layer_bitmap(
    context: &ID2D1DeviceContext,
    rect: UiRect,
    bitmap: &ID2D1Bitmap1,
    opacity: f32,
    transform: LayerTransform,
) {
    if opacity <= 0.0 {
        return;
    }
    if transform.is_identity() {
        draw_bitmap_opacity(context, rect, bitmap, opacity);
        return;
    }

    let mut previous = windows_numerics::Matrix3x2::identity();
    let layer_transform = d2d_layer_transform(rect, transform);
    unsafe {
        context.GetTransform(&mut previous);
        let combined = layer_transform * previous;
        context.SetTransform(&combined);
        draw_bitmap_opacity(context, rect, bitmap, opacity);
        context.SetTransform(&previous);
    }
}

fn d2d_layer_transform(rect: UiRect, transform: LayerTransform) -> windows_numerics::Matrix3x2 {
    let center = windows_numerics::Vector2 {
        X: rect.left as f32 + rect.width() as f32 * transform.origin_x(),
        Y: rect.top as f32 + rect.height() as f32 * transform.origin_y(),
    };
    windows_numerics::Matrix3x2::scale_around(transform.scale_x(), transform.scale_y(), center)
        * windows_numerics::Matrix3x2::rotation_around(transform.rotation_degrees_f32(), center)
        * windows_numerics::Matrix3x2::translation(
            transform.translation_x(),
            transform.translation_y(),
        )
}

fn polygon_points(path: &UiPath) -> Option<Vec<lgui::core::Point>> {
    let mut points = Vec::new();
    let mut has_close = false;
    for command in path.commands() {
        match *command {
            UiPathCommand::MoveTo(point) | UiPathCommand::LineTo(point) => points.push(point),
            UiPathCommand::Close => {
                has_close = true;
            }
            UiPathCommand::QuadraticTo { .. } | UiPathCommand::CubicTo { .. } => return None,
        }
    }
    if has_close && points.len() >= 3 {
        Some(points)
    } else {
        None
    }
}

fn mask_polygon(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    rect: UiRect,
    points: &[lgui::core::Point],
) {
    for y in 0..height {
        for x in 0..width {
            if !point_in_polygon(rect.left + x, rect.top + y, points) {
                let index = ((y * width + x) * 4) as usize;
                pixels[index..index + 4].fill(0);
            }
        }
    }
}

fn point_in_polygon(x: i32, y: i32, points: &[lgui::core::Point]) -> bool {
    let mut inside = false;
    let mut previous = points.len() - 1;
    for current in 0..points.len() {
        let a = points[current];
        let b = points[previous];
        if (a.y > y) != (b.y > y) {
            let intersection_x =
                (b.x - a.x) as f32 * (y - a.y) as f32 / (b.y - a.y) as f32 + a.x as f32;
            if (x as f32) < intersection_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn backdrop_blur_signature(rect: UiRect, style: lgui::core::BackdropBlurStyle) -> u64 {
    let mut hasher = DefaultHasher::new();
    "backdrop-blur-bitmap".hash(&mut hasher);
    style.source.hash(&mut hasher);
    style.fit.hash(&mut hasher);
    rect.left.hash(&mut hasher);
    rect.top.hash(&mut hasher);
    rect.right.hash(&mut hasher);
    rect.bottom.hash(&mut hasher);
    style.source_rect.left.hash(&mut hasher);
    style.source_rect.top.hash(&mut hasher);
    style.source_rect.right.hash(&mut hasher);
    style.source_rect.bottom.hash(&mut hasher);
    style.radius.hash(&mut hasher);
    style.tint.0.hash(&mut hasher);
    style.tint_alpha.to_bits().hash(&mut hasher);
    hasher.finish()
}

fn draw_text(
    context: &ID2D1DeviceContext,
    dwrite_factory: &IDWriteFactory,
    rect: UiRect,
    text: &str,
    style: lgui::core::TextStyle,
) -> Result<()> {
    if style.alpha == 0 || text.is_empty() {
        return Ok(());
    }

    let brush = solid_brush(context, style.color, style.alpha)?;
    let text_wide: Vec<u16> = text.encode_utf16().collect();
    let locale = HSTRING::from("zh-cn");
    let format = unsafe {
        dwrite_factory.CreateTextFormat(
            ui_font_family(),
            None,
            DWRITE_FONT_WEIGHT(style.weight),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            style.height.unsigned_abs() as f32,
            &locale,
        )?
    };
    let alignment = match style.align {
        TextAlign::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
        TextAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
        TextAlign::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
    };

    unsafe {
        format.SetTextAlignment(alignment)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        apply_dwrite_font_fallback(dwrite_factory, &format)?;
        let layout = dwrite_factory.CreateTextLayout(
            &text_wide,
            &format,
            rect.width().max(1) as f32,
            rect.height().max(1) as f32,
        )?;
        if style.tracking != 0 {
            if let Ok(layout1) = layout.cast::<IDWriteTextLayout1>() {
                layout1.SetCharacterSpacing(
                    0.0,
                    style.tracking as f32,
                    0.0,
                    DWRITE_TEXT_RANGE {
                        startPosition: 0,
                        length: text_wide.len() as u32,
                    },
                )?;
            }
        }

        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics)?;
        let x = rect.left as f32;
        let y =
            (rect.top as f32 + ((rect.height() as f32 - metrics.height).max(0.0) / 2.0)).round();
        context.DrawTextLayout(
            windows_numerics::Vector2 { X: x, Y: y },
            &layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
    }
    Ok(())
}

fn solid_brush(
    context: &ID2D1DeviceContext,
    color: Color,
    alpha: u8,
) -> Result<ID2D1SolidColorBrush> {
    unsafe { context.CreateSolidColorBrush(&d2d_color(color, alpha), None) }
}

fn d2d_rect(rect: UiRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: rect.left as f32,
        top: rect.top as f32,
        right: rect.right as f32,
        bottom: rect.bottom as f32,
    }
}

fn vector2(x: i32, y: i32) -> windows_numerics::Vector2 {
    windows_numerics::Vector2 {
        X: x as f32,
        Y: y as f32,
    }
}

fn rounded_rect(rect: UiRect, radius: i32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: d2d_rect(rect),
        radiusX: radius as f32,
        radiusY: radius as f32,
    }
}

fn d2d_color(color: Color, alpha: u8) -> D2D1_COLOR_F {
    d2d_color_alpha(color, alpha as f32 / 255.0)
}

fn d2d_color_alpha(color: Color, alpha: f32) -> D2D1_COLOR_F {
    let rgb = color.0;
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: alpha.clamp(0.0, 1.0),
    }
}

fn transparent() -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

fn opaque_black() -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
}

fn static_layer_clear_color(spec: &StaticLayerSpec) -> D2D1_COLOR_F {
    match spec.background {
        StaticLayerBackground::Opaque => opaque_black(),
        StaticLayerBackground::Transparent => transparent(),
    }
}

fn create_scene_bitmap(
    context: &ID2D1DeviceContext,
    width: i32,
    height: i32,
) -> Result<ID2D1Bitmap1> {
    create_bitmap_with_options(context, width, height, D2D1_BITMAP_OPTIONS_TARGET, None)
}

fn create_bitmap_with_options(
    context: &ID2D1DeviceContext,
    width: i32,
    height: i32,
    options: D2D1_BITMAP_OPTIONS,
    pixels: Option<&[u8]>,
) -> Result<ID2D1Bitmap1> {
    unsafe {
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: options,
            colorContext: ManuallyDrop::<Option<ID2D1ColorContext>>::new(None),
        };
        match pixels {
            Some(pixels) => context.CreateBitmap(
                D2D_SIZE_U {
                    width: width.max(1) as u32,
                    height: height.max(1) as u32,
                },
                Some(pixels.as_ptr().cast()),
                (width.max(1) * 4) as u32,
                &props,
            ),
            None => context.CreateBitmap(
                D2D_SIZE_U {
                    width: width.max(1) as u32,
                    height: height.max(1) as u32,
                },
                None,
                0,
                &props,
            ),
        }
    }
}

fn static_layer_cache_key(
    id: &UiId,
    spec: &StaticLayerSpec,
    width: i32,
    height: i32,
    child_signature: u64,
) -> D2dBitmapCacheKey {
    D2dBitmapCacheKey::StaticLayer {
        raster_key: static_layer_raster_cache::cache_key(id, spec, width, height, child_signature),
        id: id.clone(),
        spec_signature: static_layer_spec_signature(spec),
        child_signature,
        width,
        height,
    }
}

fn static_layer_spec_signature(spec: &StaticLayerSpec) -> u64 {
    let mut hasher = DefaultHasher::new();
    spec.cache_signature().hash(&mut hasher);
    hasher.finish()
}

fn trace_d2d_regions(label: &str, rects: Option<&[UiRect]>) {
    if !trace::enabled(TraceCategory::RegionDetail) {
        return;
    }
    match rects {
        Some(rects) => {
            let area: i64 = rects
                .iter()
                .map(|rect| rect.width().max(0) as i64 * rect.height().max(0) as i64)
                .sum();
            eprintln!(
                "[ui-trace] {label}: rects={} area={} rect_list={:?}",
                rects.len(),
                area,
                rects
            );
        }
        None => eprintln!("[ui-trace] {label}: mode=full"),
    }
}

fn trace_duration(label: &str, duration: Duration) {
    if trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}

fn trace_custom_duration(label: &str, key: &str, duration: Duration) {
    if trace::duration_detail_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: key={key} {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d2d_matrix_matches_backend_neutral_layer_transform() {
        let rect = UiRect::new(10, 20, 30, 60);
        let transform = LayerTransform::identity()
            .scale_xy(2.0, 1.0)
            .rotation_degrees(90.0)
            .translation(75.0, -25.0)
            .origin(0.5, 0.5);
        let matrix = d2d_layer_transform(rect, transform);
        let apply = |x: f32, y: f32| {
            (
                x * matrix.M11 + y * matrix.M21 + matrix.M31,
                x * matrix.M12 + y * matrix.M22 + matrix.M32,
            )
        };
        let actual = apply(rect.left as f32, rect.top as f32);
        let expected = transform.transform_point(rect, rect.left as f32, rect.top as f32);
        assert!((actual.0 - expected.0).abs() < 0.001);
        assert!((actual.1 - expected.1).abs() < 0.001);
    }

    #[test]
    fn bitmap_cache_budget_evicts_oldest_entries_and_keeps_one_oversized_entry() {
        let first = image_cache_key(
            UiRect::new(0, 0, 10, 10),
            &UiImageSource::Static("first"),
            ImageFit::Fill,
        );
        let second = image_cache_key(
            UiRect::new(0, 0, 20, 10),
            &UiImageSource::Static("second"),
            ImageFit::Fill,
        );
        let third = image_cache_key(
            UiRect::new(0, 0, 30, 10),
            &UiImageSource::Static("third"),
            ImageFit::Fill,
        );
        let entries = vec![
            (first.clone(), 1, first.estimated_bytes()),
            (second.clone(), 2, second.estimated_bytes()),
            (third.clone(), 3, third.estimated_bytes()),
        ];
        let total = entries.iter().map(|(_, _, bytes)| bytes).sum();

        let evictions = bitmap_cache_eviction_plan(entries, total, third.estimated_bytes());

        assert_eq!(evictions, vec![first, second]);
        assert!(bitmap_cache_eviction_plan(
            vec![(third.clone(), 1, third.estimated_bytes())],
            third.estimated_bytes(),
            1,
        )
        .is_empty());
        assert_eq!(
            d2d_bitmap_cache_budget(1432, 860),
            D2D_BITMAP_CACHE_MIN_BUDGET_BYTES
        );
        assert!(d2d_bitmap_cache_budget(3840, 2160) > D2D_BITMAP_CACHE_MIN_BUDGET_BYTES);
    }

    #[test]
    fn native_radial_gradient_stops_follow_the_quadratic_falloff() {
        let stops = radial_gradient_stops(lgui::core::RadialGradientLayer::new(
            Color(0x336699),
            0.8,
            0.5,
            0.5,
            0.5,
        ));

        assert_eq!(stops.map(|stop| stop.position), [0.0, 0.25, 0.5, 0.75, 1.0]);
        assert!((stops[0].color.a - 0.8).abs() < 0.0001);
        assert!((stops[2].color.a - 0.2).abs() < 0.0001);
        assert_eq!(stops[4].color.a, 0.0);
    }

    #[test]
    fn overlay_brush_cache_reachability_tracks_style_and_rect() {
        let rect = UiRect::new(0, 0, 320, 180);
        let style = OverlayStyle::new()
            .vertical(lgui::core::VerticalGradientLayer::new(
                Color(0x112233),
                0.1,
                0.4,
            ))
            .radial(lgui::core::RadialGradientLayer::new(
                Color(0x445566),
                0.2,
                0.5,
                0.5,
                0.6,
            ));
        let overlay = |rect, style| ScenePrimitive::Overlay {
            id: UiId::owned("overlay".to_string()),
            rect,
            style,
            phase: lgui::core::RenderPhase::Content,
        };

        let keys = overlay_brush_cache_keys(&[overlay(rect, style.clone())]);
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&overlay_brush_cache_key(rect, &style)));

        let moved_keys = overlay_brush_cache_keys(&[overlay(rect.translate(10, 0), style.clone())]);
        let changed_style = style.radial(lgui::core::RadialGradientLayer::new(
            Color(0x778899),
            0.3,
            0.4,
            0.4,
            0.5,
        ));
        let changed_keys = overlay_brush_cache_keys(&[overlay(rect, changed_style)]);

        assert!(keys.is_disjoint(&moved_keys));
        assert!(keys.is_disjoint(&changed_keys));
    }

    #[test]
    fn transparent_pure_image_static_layer_reuses_the_image_cache_key() {
        let rect = UiRect::new(0, 0, 320, 180);
        let spec = StaticLayerSpec::new(StaticLayerSource::hybrid(
            Some("background"),
            ImageFit::Cover,
        ))
        .cache_policy(StaticLayerCachePolicy::Memory)
        .transparent_background();
        assert_eq!(
            pure_static_layer_image(&spec, &[]),
            Some(("background", ImageFit::Cover))
        );

        let mut keys = HashSet::new();
        collect_bitmap_cache_keys(
            &[ScenePrimitive::StaticLayer {
                id: UiId::owned("static".to_string()),
                rect,
                spec: spec.clone(),
                commands: Vec::new(),
                child_signature: 7,
                phase: lgui::core::RenderPhase::Content,
            }],
            &mut keys,
        );

        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&image_cache_key(
            UiRect::new(0, 0, rect.width(), rect.height()),
            &UiImageSource::Static("background"),
            ImageFit::Cover,
        )));
        assert!(pure_static_layer_image(
            &StaticLayerSpec::new(StaticLayerSource::baked("background", ImageFit::Cover)),
            &[],
        )
        .is_none());
    }

    #[test]
    fn bitmap_cache_reachability_replaces_keys_from_the_previous_scene() {
        let image = |name| ScenePrimitive::Image {
            id: UiId::owned(format!("{name}-image")),
            rect: UiRect::new(0, 0, 64, 64),
            source: UiImageSource::Static(name),
            fit: ImageFit::Cover,
            phase: lgui::core::RenderPhase::Content,
        };

        let login_keys = bitmap_cache_keys(&[image("login")]);
        let lobby_keys = bitmap_cache_keys(&[image("lobby")]);

        assert_eq!(login_keys.len(), 1);
        assert_eq!(lobby_keys.len(), 1);
        assert!(login_keys.is_disjoint(&lobby_keys));
        assert!(lobby_keys.contains(&image_cache_key(
            UiRect::new(0, 0, 64, 64),
            &UiImageSource::Static("lobby"),
            ImageFit::Cover,
        )));
    }
}
