use super::*;

pub(super) fn draw_command_d2d(
    resources: &mut D2dRenderer,
    command: &ScenePrimitive,
) -> Result<()> {
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
            rect,
            source,
            request,
            fit,
            ..
        } => {
            let _ = crate::assets::request_image(request);
            draw_image(resources, *rect, source, *fit)
        }
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
                let provider = crate::assets::render_resources().custom_paint().cloned();
                if let Some(provider) = provider {
                    if let Some(fragment) =
                        provider.record(key, *rect, *style).map_err(|error| {
                            Error::new(HRESULT(0x80004005_u32 as i32), error.to_string())
                        })?
                    {
                        draw_commands_d2d(resources, fragment.commands(), Some(*rect))
                    } else {
                        Ok(())
                    }
                } else {
                    Ok(())
                }
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
                .map(|command| translate_command(command, 0.0, -spec.scroll_y))
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

pub(super) fn draw_rect(
    context: &ID2D1DeviceContext,
    rect: UiRect,
    style: VisualStyle,
) -> Result<()> {
    unsafe {
        if let Some(fill) = style.fill {
            let brush = solid_brush(context, fill, style.fill_alpha)?;
            if style.radius > 0.0 {
                let rounded = rounded_rect(rect, style.radius);
                context.FillRoundedRectangle(&rounded, &brush);
            } else {
                let rect = d2d_rect(rect);
                context.FillRectangle(&rect, &brush);
            }
        }
        if let Some(stroke) = style.stroke {
            let brush = solid_brush(context, stroke.color, stroke.alpha)?;
            if style.radius > 0.0 {
                let rounded = rounded_rect(rect, style.radius);
                context.DrawRoundedRectangle(&rounded, &brush, stroke.width, None);
            } else {
                let rect = d2d_rect(rect);
                context.DrawRectangle(&rect, &brush, stroke.width, None);
            }
        }
    }
    Ok(())
}

pub(super) fn draw_ellipse(
    context: &ID2D1DeviceContext,
    rect: UiRect,
    style: VisualStyle,
) -> Result<()> {
    let ellipse = D2D1_ELLIPSE {
        point: windows_numerics::Vector2 {
            X: (rect.left + rect.right) / 2.0,
            Y: (rect.top + rect.bottom) / 2.0,
        },
        radiusX: rect.width() / 2.0,
        radiusY: rect.height() / 2.0,
    };
    unsafe {
        if let Some(fill) = style.fill {
            let brush = solid_brush(context, fill, style.fill_alpha)?;
            context.FillEllipse(&ellipse, &brush);
        }
        if let Some(stroke) = style.stroke {
            let brush = solid_brush(context, stroke.color, stroke.alpha)?;
            context.DrawEllipse(&ellipse, &brush, stroke.width, None);
        }
    }
    Ok(())
}

pub(super) fn draw_path(
    context: &ID2D1DeviceContext,
    path: &UiPath,
    style: PathStyle,
) -> Result<()> {
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
            if stroke.alpha != 0 && stroke.width > 0.0 {
                let brush = solid_brush(context, stroke.color, stroke.alpha)?;
                context.DrawGeometry(&geometry, &brush, stroke.width, None);
            }
        }
    }
    Ok(())
}

pub(super) fn create_path_geometry(
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

pub(super) fn draw_line(
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

pub(super) fn draw_image(
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

pub(super) fn image_cache_key(
    rect: UiRect,
    source: &UiImageSource,
    fit: ImageFit,
) -> D2dBitmapCacheKey {
    let (width, height) = raster_size(rect);
    D2dBitmapCacheKey::Image {
        source: source.clone(),
        fit,
        width,
        height,
    }
}

pub(super) fn image_bitmap(
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

pub(super) fn draw_icon(
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

pub(super) fn icon_cache_key(
    rect: UiRect,
    key: &'static str,
    style: IconStyle,
) -> D2dBitmapCacheKey {
    let (width, height) = raster_size(rect);
    D2dBitmapCacheKey::Icon {
        key,
        color: style.color.0,
        alpha: style.alpha,
        width,
        height,
    }
}

pub(super) fn create_compositing_layer(
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

pub(super) fn redraw_compositing_layer(
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

pub(super) fn draw_static_layer(
    resources: &mut D2dRenderer,
    id: &UiId,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    child_signature: u64,
) -> Result<()> {
    let draw_rect = rect.translate(spec.offset_x, spec.offset_y);
    let (width, height) = raster_size(rect);
    if let Some((source, fit)) = pure_static_layer_image(spec, commands) {
        let local_rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
        if let Some(bitmap) =
            image_bitmap(resources, local_rect, &UiImageSource::Static(source), fit)?
        {
            draw_bitmap_opacity(&resources.context, draw_rect, &bitmap, spec.opacity_f32());
        }
        return Ok(());
    }
    let cache_key = static_layer_cache_key(id, spec, width, height, child_signature);
    if spec.cache_policy == RasterCachePolicy::Disabled {
        let bitmap = if let Some(bitmap) = resources.frame_bitmap_cache.get(&cache_key) {
            bitmap.clone()
        } else {
            let bitmap = render_static_layer_bitmap(resources, rect, spec, commands)?;
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
        let bitmap = render_static_layer_bitmap(resources, rect, spec, commands)?;
        resources.bitmap_cache.insert_with_policy(
            cache_key,
            bitmap.clone(),
            spec.cache_policy.retention().unwrap_or_default(),
            spec.cache_policy.priority().unwrap_or_default(),
        );
        unsafe {
            resources.context.SetTarget(&resources.scene_bitmap);
        }
        bitmap
    };
    draw_bitmap_opacity(&resources.context, draw_rect, &bitmap, spec.opacity_f32());
    Ok(())
}

pub(super) fn render_static_layer_bitmap(
    resources: &mut D2dRenderer,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
) -> Result<ID2D1Bitmap1> {
    let start = Instant::now();
    let (width, height) = raster_size(rect);
    let bitmap = create_scene_bitmap(&resources.context, width, height)?;
    unsafe {
        resources.context.SetTarget(&bitmap);
        resources.context.BeginDraw();
        resources
            .context
            .Clear(Some(&static_layer_clear_color(spec)));
    }
    let local_rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
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

pub(super) fn translate_command(command: &ScenePrimitive, dx: f32, dy: f32) -> ScenePrimitive {
    lgui::core::translate_scene_primitive_for_backend(command, dx, dy)
}
