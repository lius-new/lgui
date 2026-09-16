use super::*;
use lgui_core::core::ImageRequest;

pub(super) struct SkiaPainter<'a> {
    pub(super) cache: &'a mut SkiaCache,
}

impl SkiaPainter<'_> {
    pub(super) fn draw_commands(
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
            ScenePrimitive::Text {
                rect, text, style, ..
            } => self.cache.draw_text(canvas, *rect, text, *style),
            ScenePrimitive::Line {
                start, end, stroke, ..
            } => {
                canvas.draw_line((start.x, start.y), (end.x, end.y), &stroke_paint(*stroke));
            }
            ScenePrimitive::Path { path, style, .. } => draw_path(canvas, path, *style),
            ScenePrimitive::Image {
                rect, request, fit, ..
            } => {
                if let Some(image) = self.image(request)? {
                    draw_image(canvas, &image, *rect, *fit, None);
                }
            }
            ScenePrimitive::Icon {
                rect, key, style, ..
            } => {
                if let Some(image) = self.icon(key, *rect, *style)? {
                    let destination = sk_rect(*rect);
                    canvas.draw_image_rect(image, None, &destination, &Paint::default());
                }
            }
            ScenePrimitive::Glow {
                rect, color, alpha, ..
            } => draw_glow(canvas, *rect, *color, *alpha),
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                self.draw_backdrop(canvas, *rect, None, *style)?
            }
            ScenePrimitive::BackdropBlurPath {
                rect, path, style, ..
            } => self.draw_backdrop(canvas, *rect, Some(path), *style)?,
            ScenePrimitive::Overlay { rect, style, .. } => draw_overlay(canvas, *rect, style),
            ScenePrimitive::Custom {
                rect, key, style, ..
            } => {
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
                let content_signature =
                    lgui_core::backend::resolved_content_signature(commands, *content_signature);
                let key = format!(
                    "composite:{}:{content_signature}:{}x{}:{:?}:{:?}",
                    id.as_str(),
                    rect.width().ceil(),
                    rect.height().ceil(),
                    spec.background,
                    lgui_core::backend::compositing_shadow(spec)
                );
                let image = if let Some(image) = self.cache.get(&key) {
                    image
                } else {
                    let image = self.render_layer(
                        (rect.width(), rect.height()),
                        commands,
                        0.0,
                        0.0,
                        spec.background == CompositingLayerBackground::Opaque,
                        lgui_core::backend::compositing_shadow(spec),
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
                let cacheable = spec.cache_policy != RasterCachePolicy::Disabled;
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
                            if let Some(base) =
                                self.image(&ImageRequest::new(UiImageSource::Static(key)))?
                            {
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
                            self.cache.insert_with_policy(
                                key,
                                image,
                                spec.cache_policy.retention().unwrap_or_default(),
                                spec.cache_policy.priority().unwrap_or_default(),
                            )
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
            } => {
                self.draw_scroll_raster(canvas, id, *viewport, spec, commands, *child_signature)?
            }
            ScenePrimitive::Clip { rect, commands, .. } => {
                canvas.save();
                canvas.clip_rect(sk_rect(*rect), None, true);
                self.draw_commands(canvas, commands, Some(*rect))?;
                canvas.restore();
            }
            ScenePrimitive::ClipPath {
                rect,
                path,
                commands,
                ..
            } => {
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
        shadow: Option<lgui_core::core::ShadowStyle>,
    ) -> Result<Image, String> {
        let mut surface = layer_surface(size.0, size.1)?;
        let canvas = surface.canvas();
        canvas.clear(if opaque {
            SkColor::BLACK
        } else {
            SkColor::TRANSPARENT
        });
        canvas.translate((-offset_x, -offset_y));
        self.draw_commands(canvas, commands, None)?;
        if let Some(shadow) = shadow {
            let width = surface.width();
            let height = surface.height();
            let info = ImageInfo::new(
                (width, height),
                ColorType::BGRA8888,
                AlphaType::Premul,
                None,
            );
            let row_bytes = width as usize * 4;
            let mut pixels = vec![0; row_bytes * height as usize];
            if !surface.read_pixels(&info, &mut pixels, row_bytes, (0, 0)) {
                return Err("Skia could not read shadow alpha".to_owned());
            }
            lgui_core::backend::composite_shadow(
                &mut pixels,
                width as usize,
                height as usize,
                shadow,
            );
            return skia_safe::images::raster_from_data(&info, Data::new_copy(&pixels), row_bytes)
                .ok_or_else(|| "Skia could not create shadow surface".to_owned());
        }
        Ok(surface.image_snapshot())
    }

    fn image(&mut self, request: &ImageRequest) -> Result<Option<Image>, String> {
        let source = request.source();
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
                let Some(bytes) = lgui_core::backend::cached_image_bytes(request) else {
                    let _ = lgui_core::assets::request_image(request);
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
        style: lgui_core::core::IconStyle,
    ) -> Result<Option<Image>, String> {
        let width = rect.width().ceil().max(1.0) as i32;
        let height = rect.height().ceil().max(1.0) as i32;
        let cache_key = format!(
            "icon:{key}:{width}x{height}:{}:{}",
            style.color.0, style.alpha
        );
        if let Some(image) = self.cache.get(&cache_key) {
            return Ok(Some(image));
        }
        let Some(svg) = lgui_core::backend::resolve_svg(key) else {
            return Ok(None);
        };
        let tinted = tint_svg(&svg, style.color, style.alpha);
        let dom = skia_safe::svg::Dom::from_bytes(tinted.as_bytes(), FontMgr::default())
            .map_err(|_| format!("Skia could not parse SVG icon {key}"))?;
        let mut surface = layer_surface(width as f32, height as f32)?;
        surface.canvas().clear(SkColor::TRANSPARENT);
        let intrinsic = dom.root().intrinsic_size();
        if intrinsic.width > 0.0 && intrinsic.height > 0.0 {
            surface.canvas().scale((
                width as f32 / intrinsic.width,
                height as f32 / intrinsic.height,
            ));
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
        let Some(image) = self.image(&ImageRequest::new(UiImageSource::Static(style.source)))?
        else {
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
        id: &lgui_core::core::UiId,
        viewport: UiRect,
        spec: &lgui_core::core::ScrollRasterSpec,
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
        for tile in spec
            .prefetch_tiles
            .iter()
            .copied()
            .take(spec.max_prefetch_tiles_per_frame)
        {
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
