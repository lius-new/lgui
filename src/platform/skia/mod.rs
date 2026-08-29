use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use skia_safe::{
    surfaces, AlphaType, BlendMode, Canvas, Color as SkColor, Color4f, ColorType, Data,
    Font, FontMgr, FontStyle, Image, ImageInfo, Paint, PaintStyle, Path, PathBuilder, RRect, Rect,
    SamplingOptions, Surface, TileMode,
};
use crate::{
    application::GraphicsPreference,
    assets::render_resources,
    core::{
        BackdropBlurStyle, Color, CompositingLayerBackground, ImageFit, LayerTransform,
        PathStyle, PhysicalRect, Scene, ScenePrimitive, StaticLayerBackground,
        StaticLayerCachePolicy, StaticLayerSource, Stroke, TextAlign, TextStyle, UiImageSource,
        UiPath, UiPathCommand, UiRect, VisualStyle,
    },
    renderer::{FrameInfo, MemoryPressure},
};

pub(crate) const DEFAULT_CACHE_BUDGET: usize = 96 * 1024 * 1024;

pub fn probe_skia_support(preference: GraphicsPreference) -> Result<(), String> {
    match preference {
        GraphicsPreference::Auto | GraphicsPreference::Software => {
            surfaces::raster_n32_premul((1, 1))
                .map(|_| ())
                .ok_or_else(|| "Skia could not create a raster surface".to_owned())
        }
        #[cfg(feature = "renderer-skia-gl")]
        GraphicsPreference::OpenGl => surfaces::raster_n32_premul((1, 1))
            .map(|_| ())
            .ok_or_else(|| "Skia could not create a raster surface".to_owned()),
        #[cfg(not(feature = "renderer-skia-gl"))]
        GraphicsPreference::OpenGl => Err("OpenGL is not enabled on this target".to_owned()),
        GraphicsPreference::Vulkan => Err("Vulkan is not enabled on this build".to_owned()),
        GraphicsPreference::Metal => Err("Metal is only available on macOS".to_owned()),
    }
}

struct SkiaTextSystem;

impl crate::text::TextSystem for SkiaTextSystem {
    fn measure(
        &self,
        request: &crate::text::TextMeasureRequest<'_>,
    ) -> Option<crate::text::TextMetrics> {
        let font = skia_font(request.font_height.abs().max(1.0), request.font_weight);
        let (width, _) = font.measure_str(request.text, None);
        Some(crate::text::TextMetrics { width })
    }
}

pub(crate) fn skia_text_system_handle() -> crate::text::TextSystemHandle {
    crate::text::TextSystemHandle::new(SkiaTextSystem)
}

struct CachedImage {
    image: Image,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
pub(crate) struct SkiaCache {
    entries: HashMap<String, CachedImage>,
    resident_bytes: usize,
    budget_bytes: usize,
    generation: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SkiaCacheStats {
    pub budget_bytes: usize,
    pub resident_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl SkiaCache {
    pub(crate) fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            ..Self::default()
        }
    }

    fn get(&mut self, key: &str) -> Option<Image> {
        let Some(entry) = self.entries.get_mut(key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        entry.used = self.generation;
        Some(entry.image.clone())
    }

    fn insert(&mut self, key: String, image: Image) -> Image {
        let bytes = image.width().max(0) as usize * image.height().max(0) as usize * 4;
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.bytes);
        }
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            CachedImage {
                image: image.clone(),
                bytes,
                used: self.generation,
            },
        );
        self.evict_to_budget();
        image
    }

    pub(crate) fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn evict_to_budget(&mut self) {
        while self.resident_bytes > self.budget_bytes {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Moderate => {
                let current = self.generation;
                let before = self.entries.len();
                self.entries.retain(|_, entry| {
                    let keep = current.wrapping_sub(entry.used) <= 2;
                    if !keep {
                        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                    }
                    keep
                });
                self.evictions = self
                    .evictions
                    .saturating_add(before.saturating_sub(self.entries.len()) as u64);
            }
            MemoryPressure::Critical => {
                self.evictions = self.evictions.saturating_add(self.entries.len() as u64);
                self.entries.clear();
                self.resident_bytes = 0;
            }
        }
    }

    pub(crate) fn stats(&self) -> SkiaCacheStats {
        SkiaCacheStats {
            budget_bytes: self.budget_bytes,
            resident_bytes: self.resident_bytes,
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
        }
    }
}

pub(crate) struct SkiaSoftwareSurface {
    pixels: Vec<u8>,
    size: (i32, i32),
    cache: SkiaCache,
}

impl SkiaSoftwareSurface {
    pub(crate) fn new(cache_budget: usize) -> Self {
        Self {
            pixels: Vec::new(),
            size: (0, 0),
            cache: SkiaCache::new(cache_budget),
        }
    }

    fn ensure_surface(&mut self, width: i32, height: i32) {
        let size = (width.max(1), height.max(1));
        if self.size != size {
            self.size = size;
            self.pixels.resize(size.0 as usize * size.1 as usize * 4, 0);
            self.cache.trim(MemoryPressure::Critical);
        }
    }

    pub(crate) fn draw(&mut self, scene: &Scene, frame: &FrameInfo<'_>) -> Result<(), String> {
        let viewport = frame.viewport();
        self.ensure_surface(viewport.width(), viewport.height());
        let info = ImageInfo::new(
            self.size,
            ColorType::BGRA8888,
            AlphaType::Premul,
            None,
        );
        let mut surface = surfaces::wrap_pixels(
            &info,
            self.pixels.as_mut_slice(),
            self.size.0 as usize * 4,
            None,
        )
        .ok_or_else(|| "Skia could not wrap the retained software surface".to_owned())?;
        paint_scene_damage(surface.canvas(), &mut self.cache, scene, frame)
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn size(&self) -> (i32, i32) {
        self.size
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        self.cache.trim(pressure);
        if pressure == MemoryPressure::Critical {
            self.pixels.clear();
            self.size = (0, 0);
        }
    }

    pub(crate) fn cache_stats(&self) -> SkiaCacheStats {
        self.cache.stats()
    }
}

pub(crate) fn paint_scene_damage(
    canvas: &Canvas,
    cache: &mut SkiaCache,
    scene: &Scene,
    frame: &FrameInfo<'_>,
) -> Result<(), String> {
    cache.begin_frame();
    let clips: Vec<PhysicalRect> = if frame.is_full_redraw() {
        vec![frame.viewport()]
    } else {
        frame.damage().to_vec()
    };
    let mut painter = SkiaPainter { cache };
    for clip in clips {
        canvas.save();
        canvas.clip_rect(physical_rect(clip), None, false);
        canvas.clear(SkColor::TRANSPARENT);
        painter.draw_commands(canvas, scene.commands(), Some(ui_rect_from_physical(clip)))?;
        canvas.restore();
    }
    Ok(())
}

struct SkiaPainter<'a> {
    cache: &'a mut SkiaCache,
}

impl SkiaPainter<'_> {
    fn draw_commands(
        &mut self,
        canvas: &Canvas,
        commands: &[ScenePrimitive],
        clip: Option<UiRect>,
    ) -> Result<(), String> {
        for command in commands {
            if clip.is_some_and(|clip| clip.intersect(command.paint_bounds()).is_none()) {
                continue;
            }
            self.draw_command(canvas, command)?;
        }
        Ok(())
    }

    fn draw_command(&mut self, canvas: &Canvas, command: &ScenePrimitive) -> Result<(), String> {
        match command {
            ScenePrimitive::Rect { rect, style, .. } => draw_rect(canvas, *rect, *style),
            ScenePrimitive::Ellipse { rect, style, .. } => draw_ellipse(canvas, *rect, *style),
            ScenePrimitive::Text { rect, text, style, .. } => {
                draw_text(canvas, *rect, text, *style)
            }
            ScenePrimitive::Line { start, end, stroke, .. } => {
                canvas.draw_line((start.x, start.y), (end.x, end.y), &stroke_paint(*stroke));
            }
            ScenePrimitive::Path { path, style, .. } => draw_path(canvas, path, *style),
            ScenePrimitive::Image { rect, source, fit, .. } => {
                if let Some(image) = self.image(source)? {
                    draw_image(canvas, &image, *rect, *fit, None);
                }
            }
            ScenePrimitive::Icon { rect, key, style, .. } => {
                if let Some(image) = self.icon(key, *rect, *style)? {
                    let destination = sk_rect(*rect);
                    canvas.draw_image_rect(image, None, &destination, &Paint::default());
                }
            }
            ScenePrimitive::Glow { rect, color, alpha, .. } => {
                draw_glow(canvas, *rect, *color, *alpha)
            }
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                self.draw_backdrop(canvas, *rect, None, *style)?
            }
            ScenePrimitive::BackdropBlurPath { rect, path, style, .. } => {
                self.draw_backdrop(canvas, *rect, Some(path), *style)?
            }
            ScenePrimitive::Overlay { rect, style, .. } => draw_overlay(canvas, *rect, style),
            ScenePrimitive::Custom { rect, key, style, .. } => {
                if let Some(style) = style {
                    if let Some(fragment) = custom_scene(key, *rect, *style)? {
                        self.draw_commands(canvas, fragment.commands(), Some(*rect))?;
                    }
                }
            }
            ScenePrimitive::CompositingLayer {
                id,
                rect,
                spec,
                commands,
                content_signature,
                ..
            } => {
                let key = format!("composite:{}:{content_signature}", id.as_str());
                let image = if let Some(image) = self.cache.get(&key) {
                    image
                } else {
                    let image = self.render_layer(
                        (rect.width(), rect.height()),
                        commands,
                        0.0,
                        0.0,
                        spec.background == CompositingLayerBackground::Opaque,
                    )?;
                    self.cache.insert(key, image)
                };
                draw_composited(canvas, &image, *rect, spec.opacity, spec.transform);
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                let cacheable = spec.cache_policy != StaticLayerCachePolicy::Disabled;
                let key = format!(
                    "static:{}:{}:{}:{}x{}",
                    id.as_str(),
                    spec.revision,
                    child_signature,
                    rect.width().ceil(),
                    rect.height().ceil()
                );
                let image = if cacheable {
                    self.cache.get(&key)
                } else {
                    None
                };
                let image = match image {
                    Some(image) => image,
                    None => {
                        let mut surface = layer_surface(rect.width(), rect.height())?;
                        let layer_canvas = surface.canvas();
                        if spec.background == StaticLayerBackground::Opaque {
                            layer_canvas.clear(SkColor::BLACK);
                        } else {
                            layer_canvas.clear(SkColor::TRANSPARENT);
                        }
                        if let StaticLayerSource::BakedAsset { key, fit }
                        | StaticLayerSource::Hybrid {
                            baked_base: Some(key),
                            fit,
                        } = spec.source
                        {
                            if let Some(base) = self.image(&UiImageSource::Static(key))? {
                                draw_image(
                                    layer_canvas,
                                    &base,
                                    UiRect::new(0.0, 0.0, rect.width(), rect.height()),
                                    fit,
                                    None,
                                );
                            }
                        }
                        layer_canvas.save();
                        layer_canvas.translate((-rect.left, -rect.top));
                        self.draw_commands(layer_canvas, commands, Some(*rect))?;
                        layer_canvas.restore();
                        let image = surface.image_snapshot();
                        if cacheable {
                            self.cache.insert(key, image)
                        } else {
                            image
                        }
                    }
                };
                let destination = rect.translate(spec.offset_x, spec.offset_y);
                let mut paint = Paint::default();
                paint.set_alpha(spec.opacity);
                let destination = sk_rect(destination);
                canvas.draw_image_rect(image, None, &destination, &paint);
            }
            ScenePrimitive::ScrollRaster {
                id,
                viewport,
                spec,
                commands,
                child_signature,
                ..
            } => self.draw_scroll_raster(
                canvas,
                id,
                *viewport,
                spec,
                commands,
                *child_signature,
            )?,
            ScenePrimitive::Clip { rect, commands, .. } => {
                canvas.save();
                canvas.clip_rect(sk_rect(*rect), None, true);
                self.draw_commands(canvas, commands, Some(*rect))?;
                canvas.restore();
            }
            ScenePrimitive::ClipPath { rect, path, commands, .. } => {
                canvas.save();
                canvas.clip_path(&sk_path(path), None, true);
                self.draw_commands(canvas, commands, Some(*rect))?;
                canvas.restore();
            }
        }
        Ok(())
    }

    fn render_layer(
        &mut self,
        size: (f32, f32),
        commands: &[ScenePrimitive],
        offset_x: f32,
        offset_y: f32,
        opaque: bool,
    ) -> Result<Image, String> {
        let mut surface = layer_surface(size.0, size.1)?;
        let canvas = surface.canvas();
        canvas.clear(if opaque { SkColor::BLACK } else { SkColor::TRANSPARENT });
        canvas.translate((-offset_x, -offset_y));
        self.draw_commands(canvas, commands, None)?;
        Ok(surface.image_snapshot())
    }

    fn image(&mut self, source: &UiImageSource) -> Result<Option<Image>, String> {
        let key = image_key(source);
        if let Some(image) = self.cache.get(&key) {
            return Ok(Some(image));
        }
        let bytes: Arc<[u8]> = match source {
            UiImageSource::Static(id) => match render_resources().resolver() {
                Some(resolver) => resolver.resolve(id).map_err(|error| error.to_string())?,
                None => return Ok(None),
            },
            UiImageSource::File(_) | UiImageSource::Url(_) => {
                let Some(bytes) = crate::assets::cached_image_bytes(source) else {
                    let _ = crate::assets::request_image(source);
                    return Ok(None);
                };
                bytes
            }
            UiImageSource::Bytes { bytes, .. } => Arc::from(bytes.as_slice()),
        };
        let image = Image::from_encoded(Data::new_copy(bytes.as_ref()))
            .ok_or_else(|| format!("Skia could not decode image {key}"))?;
        Ok(Some(self.cache.insert(key, image)))
    }

    fn icon(
        &mut self,
        key: &'static str,
        rect: UiRect,
        style: crate::core::IconStyle,
    ) -> Result<Option<Image>, String> {
        let width = rect.width().ceil().max(1.0) as i32;
        let height = rect.height().ceil().max(1.0) as i32;
        let cache_key = format!("icon:{key}:{width}x{height}:{}:{}", style.color.0, style.alpha);
        if let Some(image) = self.cache.get(&cache_key) {
            return Ok(Some(image));
        }
        let Some(svg) = crate::icons::resolve_svg(key) else {
            return Ok(None);
        };
        let tinted = tint_svg(&svg, style.color, style.alpha);
        let dom = skia_safe::svg::Dom::from_bytes(tinted.as_bytes(), FontMgr::default())
            .map_err(|_| format!("Skia could not parse SVG icon {key}"))?;
        let mut surface = layer_surface(width as f32, height as f32)?;
        surface.canvas().clear(SkColor::TRANSPARENT);
        let intrinsic = dom.root().intrinsic_size();
        if intrinsic.width > 0.0 && intrinsic.height > 0.0 {
            surface
                .canvas()
                .scale((width as f32 / intrinsic.width, height as f32 / intrinsic.height));
        }
        dom.render(surface.canvas());
        Ok(Some(self.cache.insert(cache_key, surface.image_snapshot())))
    }

    fn draw_backdrop(
        &mut self,
        canvas: &Canvas,
        rect: UiRect,
        path: Option<&UiPath>,
        style: BackdropBlurStyle,
    ) -> Result<(), String> {
        let Some(image) = self.image(&UiImageSource::Static(style.source))? else {
            return Ok(());
        };
        canvas.save();
        if let Some(path) = path {
            canvas.clip_path(&sk_path(path), None, true);
        } else {
            canvas.clip_rect(sk_rect(rect), None, true);
        }
        let mut paint = Paint::default();
        paint.set_alpha_f(style.opacity.clamp(0.0, 1.0));
        paint.set_image_filter(skia_safe::image_filters::blur(
            (style.radius.max(0.0), style.radius.max(0.0)),
            TileMode::Clamp,
            None,
            None,
        ));
        draw_image(canvas, &image, style.source_rect, style.fit, Some(&paint));
        if style.tint_alpha > 0.0 {
            let mut tint = color_paint(style.tint, (style.tint_alpha * 255.0).round() as u8);
            tint.set_blend_mode(BlendMode::SrcOver);
            canvas.draw_rect(sk_rect(rect), &tint);
        }
        canvas.restore();
        Ok(())
    }

    fn draw_scroll_raster(
        &mut self,
        canvas: &Canvas,
        id: &crate::core::UiId,
        viewport: UiRect,
        spec: &crate::core::ScrollRasterSpec,
        commands: &[ScenePrimitive],
        child_signature: u64,
    ) -> Result<(), String> {
        let tile_height = spec.tile_height_px.max(1.0);
        let render_tile = |painter: &mut Self, tile_index: usize| -> Result<Image, String> {
            let tile_top = tile_index as f32 * tile_height;
            let height = (spec.content_height - tile_top).clamp(0.0, tile_height);
            let key = format!(
                "scroll:{}:{}:{}:{}:{}x{}",
                id.as_str(),
                spec.cache_epoch,
                child_signature,
                tile_index,
                viewport.width().ceil(),
                height.ceil()
            );
            if let Some(image) = painter.cache.get(&key) {
                return Ok(image);
            }
            let mut surface = layer_surface(viewport.width(), height.max(1.0))?;
            let tile_canvas = surface.canvas();
            if let Some(fill) = spec.background_fill {
                tile_canvas.clear(sk_color(fill, 255));
            } else {
                tile_canvas.clear(SkColor::TRANSPARENT);
            }
            tile_canvas.translate((-viewport.left, -(viewport.top + tile_top)));
            let content_clip = UiRect::new(
                viewport.left,
                viewport.top + tile_top,
                viewport.right,
                viewport.top + tile_top + height,
            );
            painter.draw_commands(tile_canvas, commands, Some(content_clip))?;
            Ok(painter.cache.insert(key, surface.image_snapshot()))
        };

        let prefetch_started = Instant::now();
        let prefetch_budget = Duration::from_millis(spec.max_prefetch_ms_per_frame as u64);
        for tile in spec.prefetch_tiles.iter().copied().take(spec.max_prefetch_tiles_per_frame) {
            if prefetch_budget.is_zero() || prefetch_started.elapsed() >= prefetch_budget {
                break;
            }
            let _ = render_tile(self, tile)?;
        }

        canvas.save();
        canvas.clip_rect(sk_rect(viewport), None, false);
        for tile in spec.visible_tiles.iter().copied() {
            let image = render_tile(self, tile)?;
            let tile_top = tile as f32 * tile_height;
            let top = viewport.top + tile_top - spec.scroll_y;
            let destination = Rect::new(
                viewport.left,
                top,
                viewport.right,
                top + image.height() as f32,
            );
            canvas.draw_image_rect(image, None, &destination, &Paint::default());
        }
        canvas.restore();
        Ok(())
    }
}

fn layer_surface(width: f32, height: f32) -> Result<Surface, String> {
    surfaces::raster_n32_premul((
        width.ceil().max(1.0) as i32,
        height.ceil().max(1.0) as i32,
    ))
    .ok_or_else(|| "Skia could not create an offscreen layer".to_owned())
}

fn draw_rect(canvas: &Canvas, rect: UiRect, style: VisualStyle) {
    let area = sk_rect(rect);
    if let Some(fill) = style.fill {
        let paint = color_paint(fill, style.fill_alpha);
        if style.radius > 0.0 {
            canvas.draw_rrect(RRect::new_rect_xy(area, style.radius, style.radius), &paint);
        } else {
            canvas.draw_rect(area, &paint);
        }
    }
    if let Some(stroke) = style.stroke {
        let paint = stroke_paint(stroke);
        if style.radius > 0.0 {
            canvas.draw_rrect(RRect::new_rect_xy(area, style.radius, style.radius), &paint);
        } else {
            canvas.draw_rect(area, &paint);
        }
    }
}

fn draw_ellipse(canvas: &Canvas, rect: UiRect, style: VisualStyle) {
    if let Some(fill) = style.fill {
        canvas.draw_oval(sk_rect(rect), &color_paint(fill, style.fill_alpha));
    }
    if let Some(stroke) = style.stroke {
        canvas.draw_oval(sk_rect(rect), &stroke_paint(stroke));
    }
}

fn draw_text(canvas: &Canvas, rect: UiRect, text: &str, style: TextStyle) {
    let size = style.height.abs().max(1.0);
    let font = skia_font(size, style.weight);
    let mut paint = color_paint(style.color, style.alpha);
    paint.set_anti_alias(true);
    let (width, bounds) = font.measure_str(text, Some(&paint));
    let x = match style.align {
        TextAlign::Left => rect.left,
        TextAlign::Center => rect.left + (rect.width() - width) / 2.0,
        TextAlign::Right => rect.right - width,
    };
    let y = rect.top + (rect.height() - bounds.height()) / 2.0 - bounds.top;
    canvas.draw_str(text, (x, y), &font, &paint);
}

fn skia_font(size: f32, weight: i32) -> Font {
    let family = crate::text::font_families()
        .first()
        .copied()
        .unwrap_or("Segoe UI");
    let style = FontStyle::new(
        skia_safe::font_style::Weight::from(weight.clamp(1, 1000)),
        skia_safe::font_style::Width::NORMAL,
        skia_safe::font_style::Slant::Upright,
    );
    let mut font = if let Some(typeface) = FontMgr::default().match_family_style(family, style) {
        Font::from_typeface(typeface, size)
    } else {
        let mut font = Font::default();
        font.set_size(size);
        font
    };
    font.set_subpixel(true);
    font
}

fn draw_path(canvas: &Canvas, path: &UiPath, style: PathStyle) {
    let path = sk_path(path);
    if let Some(fill) = style.fill {
        canvas.draw_path(&path, &color_paint(fill, style.fill_alpha));
    }
    if let Some(stroke) = style.stroke {
        canvas.draw_path(&path, &stroke_paint(stroke));
    }
}

fn draw_image(canvas: &Canvas, image: &Image, rect: UiRect, fit: ImageFit, paint: Option<&Paint>) {
    let image_size = (image.width() as f32, image.height() as f32);
    let destination = fitted_rect(rect, image_size, fit);
    let default_paint = Paint::default();
    if fit == ImageFit::Cover {
        canvas.save();
        canvas.clip_rect(sk_rect(rect), None, true);
    }
    canvas.draw_image_rect_with_sampling_options(
        image,
        None,
        sk_rect(destination),
        SamplingOptions::default(),
        paint.unwrap_or(&default_paint),
    );
    if fit == ImageFit::Cover {
        canvas.restore();
    }
}

fn draw_glow(canvas: &Canvas, rect: UiRect, color: Color, alpha: u8) {
    let center = (rect.left + rect.width() / 2.0, rect.top + rect.height() / 2.0);
    let colors = [sk_color_f(color, alpha as f32 / 255.0), sk_color_f(color, 0.0)];
    let positions = [0.0, 1.0];
    let gradient = skia_safe::gradient::Gradient::new(
        skia_safe::gradient::Colors::new(
            colors.as_slice(),
            Some(positions.as_slice()),
            TileMode::Clamp,
            None,
        ),
        skia_safe::gradient::Interpolation::default(),
    );
    if let Some(shader) = skia_safe::gradient::shaders::radial_gradient(
        (center, rect.width().max(rect.height()) / 2.0),
        &gradient,
        None,
    ) {
        let mut paint = Paint::default();
        paint.set_shader(shader);
        canvas.draw_rect(sk_rect(rect), &paint);
    }
}

fn draw_overlay(canvas: &Canvas, rect: UiRect, style: &crate::core::OverlayStyle) {
    for layer in &style.vertical_layers {
        let colors = [
            sk_color_f(layer.color, layer.alpha_top),
            sk_color_f(layer.color, layer.alpha_bottom),
        ];
        let gradient = skia_safe::gradient::Gradient::new(
            skia_safe::gradient::Colors::new_evenly_spaced(
                colors.as_slice(),
                TileMode::Clamp,
                None,
            ),
            skia_safe::gradient::Interpolation::default(),
        );
        if let Some(shader) = skia_safe::gradient::shaders::linear_gradient(
            ((rect.left, rect.top), (rect.left, rect.bottom)),
            &gradient,
            None,
        ) {
            let mut paint = Paint::default();
            paint.set_shader(shader);
            canvas.draw_rect(sk_rect(rect), &paint);
        }
    }
    for layer in &style.radial_layers {
        let center = (
            rect.left + rect.width() * layer.center_x,
            rect.top + rect.height() * layer.center_y,
        );
        let radius = rect.width().min(rect.height()) * layer.radius;
        let colors = [
            sk_color_f(layer.color, layer.alpha),
            sk_color_f(layer.color, 0.0),
        ];
        let gradient = skia_safe::gradient::Gradient::new(
            skia_safe::gradient::Colors::new_evenly_spaced(
                colors.as_slice(),
                TileMode::Clamp,
                None,
            ),
            skia_safe::gradient::Interpolation::default(),
        );
        if let Some(shader) = skia_safe::gradient::shaders::radial_gradient(
            (center, radius),
            &gradient,
            None,
        ) {
            let mut paint = Paint::default();
            paint.set_shader(shader);
            canvas.draw_rect(sk_rect(rect), &paint);
        }
    }
}

fn draw_composited(
    canvas: &Canvas,
    image: &Image,
    rect: UiRect,
    opacity: u8,
    transform: LayerTransform,
) {
    canvas.save();
    let origin = (
        rect.left + rect.width() * transform.origin_x(),
        rect.top + rect.height() * transform.origin_y(),
    );
    canvas.translate((origin.0 + transform.translation_x(), origin.1 + transform.translation_y()));
    canvas.rotate(transform.rotation_degrees_f32(), None);
    canvas.scale((transform.scale_x(), transform.scale_y()));
    canvas.translate((-origin.0, -origin.1));
    let mut paint = Paint::default();
    paint.set_alpha(opacity);
    let destination = sk_rect(rect);
    canvas.draw_image_rect(image, None, &destination, &paint);
    canvas.restore();
}

fn custom_scene(
    key: &str,
    rect: UiRect,
    style: crate::core::CustomPaintStyle,
) -> Result<Option<crate::assets::SceneFragment>, String> {
    let Some(provider) = render_resources().custom_paint().cloned() else {
        return Ok(None);
    };
    provider
        .record(key, rect, style)
        .map_err(|error| error.to_string())
}

fn fitted_rect(bounds: UiRect, image: (f32, f32), fit: ImageFit) -> UiRect {
    if fit == ImageFit::Fill || image.0 <= 0.0 || image.1 <= 0.0 {
        return bounds;
    }
    let scale = match fit {
        ImageFit::Contain => (bounds.width() / image.0).min(bounds.height() / image.1),
        ImageFit::Cover => (bounds.width() / image.0).max(bounds.height() / image.1),
        ImageFit::Fill => 1.0,
    };
    let width = image.0 * scale;
    let height = image.1 * scale;
    UiRect::new(
        bounds.left + (bounds.width() - width) / 2.0,
        bounds.top + (bounds.height() - height) / 2.0,
        bounds.left + (bounds.width() + width) / 2.0,
        bounds.top + (bounds.height() + height) / 2.0,
    )
}

fn sk_path(path: &UiPath) -> Path {
    let mut result = PathBuilder::new();
    for command in path.commands() {
        match command {
            UiPathCommand::MoveTo(point) => {
                result.move_to((point.x, point.y));
            }
            UiPathCommand::LineTo(point) => {
                result.line_to((point.x, point.y));
            }
            UiPathCommand::QuadraticTo { control, to } => {
                result.quad_to((control.x, control.y), (to.x, to.y));
            }
            UiPathCommand::CubicTo { control1, control2, to } => {
                result.cubic_to(
                    (control1.x, control1.y),
                    (control2.x, control2.y),
                    (to.x, to.y),
                );
            }
            UiPathCommand::Close => {
                result.close();
            }
        }
    }
    result.detach()
}

fn stroke_paint(stroke: Stroke) -> Paint {
    let mut paint = color_paint(stroke.color, stroke.alpha);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(stroke.width.max(0.0));
    paint
}

fn color_paint(color: Color, alpha: u8) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(sk_color(color, alpha));
    paint
}

fn sk_color(color: Color, alpha: u8) -> SkColor {
    SkColor::from_argb(
        alpha,
        ((color.0 >> 16) & 0xFF) as u8,
        ((color.0 >> 8) & 0xFF) as u8,
        (color.0 & 0xFF) as u8,
    )
}

fn sk_color_f(color: Color, alpha: f32) -> Color4f {
    Color4f::new(
        ((color.0 >> 16) & 0xFF) as f32 / 255.0,
        ((color.0 >> 8) & 0xFF) as f32 / 255.0,
        (color.0 & 0xFF) as f32 / 255.0,
        alpha.clamp(0.0, 1.0),
    )
}

fn sk_rect(rect: UiRect) -> Rect {
    Rect::new(rect.left, rect.top, rect.right, rect.bottom)
}

fn physical_rect(rect: PhysicalRect) -> Rect {
    Rect::new(rect.left as f32, rect.top as f32, rect.right as f32, rect.bottom as f32)
}

fn ui_rect_from_physical(rect: PhysicalRect) -> UiRect {
    UiRect::new(rect.left as f32, rect.top as f32, rect.right as f32, rect.bottom as f32)
}

fn image_key(source: &UiImageSource) -> String {
    match source {
        UiImageSource::Static(key) => format!("asset:{key}"),
        UiImageSource::File(path) => format!("file:{}", path.display()),
        UiImageSource::Url(url) => format!("url:{url}"),
        UiImageSource::Bytes { key, version, .. } => format!("bytes:{key}:{version}"),
    }
}

fn tint_svg(svg: &str, color: Color, alpha: u8) -> String {
    let hex = format!("#{:06X}", color.0 & 0xFFFFFF);
    svg.replace("currentColor", &hex)
        .replace("currentOpacity", &format!("{:.6}", alpha as f32 / 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assets::{AssetBytes, AssetError, AssetResolver, CustomPaintProvider, RenderResources, SceneFragment},
        core::{
            CompositingLayerSpec, CustomPaintStyle, IconStyle, OverlayStyle, Point,
            RadialGradientLayer, RenderPhase, ScenePrimitiveKind, ScrollRasterSpec,
            StaticLayerSpec, StaticLayerSource, UiId, UiPathCommand, UiScale,
            VerticalGradientLayer,
        },
        renderer::FrameReason,
    };

    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x04, 0x00, 0x00, 0x00, 0xB5, 0x1C, 0x0C, 0x02, 0x00, 0x00, 0x00,
        0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xFF, 0x1F, 0x00,
        0x02, 0xEB, 0x01, 0xF5, 0x8F, 0x59, 0x97, 0xDB, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    struct TestAssets;

    impl AssetResolver for TestAssets {
        fn resolve(&self, id: &str) -> Result<AssetBytes, AssetError> {
            (id == "test.pixel")
                .then(|| Arc::<[u8]>::from(PIXEL_PNG))
                .ok_or_else(|| AssetError::NotFound(id.to_owned()))
        }
    }

    struct TestCustomPaint;

    impl CustomPaintProvider for TestCustomPaint {
        fn record(
            &self,
            key: &str,
            bounds: UiRect,
            _style: CustomPaintStyle,
        ) -> Result<Option<SceneFragment>, AssetError> {
            Ok((key == "test.custom").then(|| {
                SceneFragment::new(vec![ScenePrimitive::Rect {
                    id: test_id("custom.fragment"),
                    rect: bounds,
                    style: VisualStyle::filled(Color(0x44CC88)),
                    phase: RenderPhase::Content,
                }])
            }))
        }
    }

    fn test_id(name: &str) -> UiId {
        UiId::from_parts(["skia-test", name])
    }

    fn rect_command(name: &str, rect: UiRect, color: Color) -> ScenePrimitive {
        ScenePrimitive::Rect {
            id: test_id(name),
            rect,
            style: VisualStyle::filled(color),
            phase: RenderPhase::Content,
        }
    }

    fn triangle(rect: UiRect) -> UiPath {
        UiPath::new([
            UiPathCommand::MoveTo(Point::new(rect.left, rect.bottom)),
            UiPathCommand::LineTo(Point::new(rect.left + rect.width() / 2.0, rect.top)),
            UiPathCommand::LineTo(Point::new(rect.right, rect.bottom)),
            UiPathCommand::Close,
        ])
    }

    fn draw_scene(surface: &mut SkiaSoftwareSurface, scene: &Scene, full: bool, damage: &[PhysicalRect]) {
        let frame = FrameInfo::new(
            PhysicalRect::new(0, 0, 32, 32),
            damage,
            UiScale::ONE,
            FrameReason::SceneChange,
            full,
        );
        let resources = RenderResources::new()
            .with_resolver(TestAssets)
            .with_custom_paint(TestCustomPaint);
        crate::assets::with_render_resources(resources, || {
            surface.draw(scene, &frame).expect("draw conformance scene")
        });
    }

    fn pixel(surface: &SkiaSoftwareSurface, x: usize, y: usize) -> [u8; 4] {
        let (width, _) = surface.size();
        let offset = (y * width as usize + x) * 4;
        surface.pixels()[offset..offset + 4].try_into().unwrap()
    }

    fn primitive_inventory() -> Vec<ScenePrimitive> {
        let rect = UiRect::new(4.0, 4.0, 20.0, 20.0);
        let child = vec![rect_command("child", rect, Color(0x33AAEE))];
        vec![
            rect_command("rect", rect, Color(0xFF0000)),
            ScenePrimitive::Ellipse {
                id: test_id("ellipse"),
                rect,
                style: VisualStyle::filled(Color(0x00FF00)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Text {
                id: test_id("text"),
                rect,
                text: "Skia".into(),
                style: TextStyle::new(Color::WHITE, 12.0, 400),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Custom {
                id: test_id("custom"),
                rect,
                key: "test.custom",
                style: Some(CustomPaintStyle::new(Color::WHITE, 1.0)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Line {
                id: test_id("line"),
                start: Point::new(4.0, 4.0),
                end: Point::new(20.0, 20.0),
                stroke: Stroke::new(Color::WHITE, 2.0, 255),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Path {
                id: test_id("path"),
                rect,
                path: triangle(rect),
                style: PathStyle::filled(Color(0xFFCC00)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Image {
                id: test_id("image"),
                rect,
                source: UiImageSource::bytes("test.pixel", 1, Arc::new(PIXEL_PNG.to_vec())),
                fit: ImageFit::Fill,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Icon {
                id: test_id("icon"),
                rect,
                key: "copy",
                style: IconStyle::new(Color::WHITE),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Glow {
                id: test_id("glow"),
                rect,
                color: Color(0x33AAFF),
                alpha: 180,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::BackdropBlur {
                id: test_id("blur"),
                rect,
                style: BackdropBlurStyle::new("test.pixel", ImageFit::Fill, rect).radius(2.0),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::BackdropBlurPath {
                id: test_id("blur-path"),
                rect,
                path: triangle(rect),
                style: BackdropBlurStyle::new("test.pixel", ImageFit::Fill, rect).radius(2.0),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Overlay {
                id: test_id("overlay"),
                rect,
                style: OverlayStyle::new()
                    .vertical(VerticalGradientLayer::new(Color::WHITE, 0.8, 0.1))
                    .radial(RadialGradientLayer::new(Color(0x44AAFF), 0.7, 0.5, 0.5, 0.5)),
                phase: RenderPhase::Content,
            },
            ScenePrimitive::CompositingLayer {
                id: test_id("compositing"),
                rect,
                spec: CompositingLayerSpec::new().opacity(0.75),
                commands: child.clone(),
                content_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::StaticLayer {
                id: test_id("static"),
                rect,
                spec: StaticLayerSpec::new(StaticLayerSource::runtime()).transparent_background(),
                commands: child.clone(),
                child_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::ScrollRaster {
                id: test_id("scroll"),
                viewport: rect,
                spec: ScrollRasterSpec {
                    cache_epoch: 1,
                    content_height: rect.height(),
                    scroll_y: 0.0,
                    tile_height_px: rect.height(),
                    memory_budget_bytes: 1024 * 1024,
                    background_fill: None,
                    visible_tiles: vec![0],
                    prefetch_tiles: Vec::new(),
                    max_prefetch_tiles_per_frame: 0,
                    max_prefetch_ms_per_frame: 0,
                },
                commands: child.clone(),
                child_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::Clip {
                id: test_id("clip"),
                rect,
                commands: child.clone(),
                child_signature: 1,
                phase: RenderPhase::Content,
            },
            ScenePrimitive::ClipPath {
                id: test_id("clip-path"),
                rect,
                path: triangle(rect),
                commands: child,
                child_signature: 1,
                phase: RenderPhase::Content,
            },
        ]
    }

    #[test]
    fn software_probe_creates_a_real_skia_surface() {
        assert!(probe_skia_support(GraphicsPreference::Software).is_ok());
    }

    #[test]
    fn gpu_probe_reports_only_compiled_drivers() {
        assert_eq!(
            probe_skia_support(GraphicsPreference::OpenGl).is_ok(),
            cfg!(feature = "renderer-skia-gl")
        );
        assert!(probe_skia_support(GraphicsPreference::Vulkan).is_err());
        assert!(probe_skia_support(GraphicsPreference::Metal).is_err());
    }

    #[test]
    fn cache_is_byte_bounded() {
        let mut cache = SkiaCache::new(4 * 4 * 4);
        cache.begin_frame();
        let first = layer_surface(4.0, 4.0).unwrap().image_snapshot();
        cache.insert("first".to_owned(), first);
        cache.begin_frame();
        let second = layer_surface(4.0, 4.0).unwrap().image_snapshot();
        cache.insert("second".to_owned(), second);
        assert!(cache.resident_bytes <= cache.budget_bytes);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn image_fit_preserves_aspect_ratio() {
        assert_eq!(
            fitted_rect(UiRect::new(0.0, 0.0, 100.0, 100.0), (200.0, 100.0), ImageFit::Contain),
            UiRect::new(0.0, 25.0, 100.0, 75.0)
        );
    }

    #[test]
    fn every_scene_primitive_has_a_real_skia_paint_path() {
        let commands = primitive_inventory();
        let kinds = commands.iter().map(ScenePrimitive::kind).collect::<Vec<_>>();
        assert_eq!(kinds, ScenePrimitiveKind::ALL);
        for command in commands {
            let mut scene = Scene::new();
            scene.push(command);
            let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
            draw_scene(
                &mut surface,
                &scene,
                true,
                &[PhysicalRect::new(0, 0, 32, 32)],
            );
        }
    }

    #[test]
    fn dirty_draw_preserves_pixels_outside_damage_and_clears_removals() {
        let full = [PhysicalRect::new(0, 0, 32, 32)];
        let left = [PhysicalRect::new(0, 0, 16, 32)];
        let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
        let mut red = Scene::new();
        red.push(rect_command(
            "background-red",
            UiRect::new(0.0, 0.0, 32.0, 32.0),
            Color(0xFF0000),
        ));
        draw_scene(&mut surface, &red, true, &full);
        let red_pixel = pixel(&surface, 24, 16);

        let mut blue = Scene::new();
        blue.push(rect_command(
            "background-blue",
            UiRect::new(0.0, 0.0, 32.0, 32.0),
            Color(0x0000FF),
        ));
        draw_scene(&mut surface, &blue, false, &left);
        assert_ne!(pixel(&surface, 8, 16), red_pixel);
        assert_eq!(pixel(&surface, 24, 16), red_pixel);

        draw_scene(&mut surface, &Scene::new(), false, &left);
        assert_eq!(pixel(&surface, 8, 16), [0, 0, 0, 0]);
        assert_eq!(pixel(&surface, 24, 16), red_pixel);
    }

    #[test]
    fn dpi_projection_and_nested_clip_use_physical_bounds() {
        let mut logical = Scene::new();
        logical.push(ScenePrimitive::Clip {
            id: test_id("outer-clip"),
            rect: UiRect::new(0.0, 0.0, 5.0, 5.0),
            commands: vec![ScenePrimitive::Clip {
                id: test_id("inner-clip"),
                rect: UiRect::new(2.0, 2.0, 5.0, 5.0),
                commands: vec![rect_command(
                    "clip-fill",
                    UiRect::new(0.0, 0.0, 8.0, 8.0),
                    Color::WHITE,
                )],
                child_signature: 1,
                phase: RenderPhase::Content,
            }],
            child_signature: 1,
            phase: RenderPhase::Content,
        });
        let scene = logical.project_to_physical(UiScale::new(2.0));
        let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
        draw_scene(
            &mut surface,
            &scene,
            true,
            &[PhysicalRect::new(0, 0, 32, 32)],
        );
        assert_eq!(pixel(&surface, 2, 2), [0, 0, 0, 0]);
        assert_ne!(pixel(&surface, 6, 6), [0, 0, 0, 0]);
        assert_eq!(pixel(&surface, 12, 12), [0, 0, 0, 0]);
    }

    #[test]
    fn cache_trim_is_deterministic_at_both_pressure_levels() {
        let mut cache = SkiaCache::new(1024 * 1024);
        cache.begin_frame();
        cache.insert(
            "old".to_owned(),
            layer_surface(4.0, 4.0).unwrap().image_snapshot(),
        );
        for _ in 0..4 {
            cache.begin_frame();
        }
        cache.insert(
            "recent".to_owned(),
            layer_surface(4.0, 4.0).unwrap().image_snapshot(),
        );
        cache.trim(MemoryPressure::Moderate);
        assert!(!cache.entries.contains_key("old"));
        assert!(cache.entries.contains_key("recent"));
        cache.trim(MemoryPressure::Critical);
        assert!(cache.entries.is_empty());
        assert_eq!(cache.resident_bytes, 0);
    }

    #[test]
    fn layer_opacity_and_content_signature_invalidate_retained_images() {
        let bounds = UiRect::new(0.0, 0.0, 16.0, 16.0);
        let layer = |signature, color| ScenePrimitive::CompositingLayer {
            id: test_id("retained-layer"),
            rect: bounds,
            spec: CompositingLayerSpec::new().opacity(0.5),
            commands: vec![rect_command("retained-child", bounds, color)],
            content_signature: signature,
            phase: RenderPhase::Content,
        };
        let damage = [PhysicalRect::new(0, 0, 32, 32)];
        let mut surface = SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET);
        let mut first = Scene::new();
        first.push(layer(1, Color(0xFF0000)));
        draw_scene(&mut surface, &first, true, &damage);
        let red = pixel(&surface, 8, 8);
        assert!(red[3] >= 126 && red[3] <= 129);

        let mut second = Scene::new();
        second.push(layer(2, Color(0x0000FF)));
        draw_scene(&mut surface, &second, true, &damage);
        let blue = pixel(&surface, 8, 8);
        assert_ne!(blue, red);
        assert!(surface.cache_stats().misses >= 2);
    }
}
