use super::*;

pub(super) fn draw_overlay(
    resources: &mut D2dRenderer,
    rect: UiRect,
    style: &OverlayStyle,
) -> Result<()> {
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

pub(super) fn ensure_overlay_brush_set(
    resources: &mut D2dRenderer,
    rect: UiRect,
    style: &OverlayStyle,
) -> Result<D2dOverlayBrushCacheKey> {
    let key = overlay_brush_cache_key(rect, style);
    if resources.overlay_brush_cache.contains_key(&key) {
        return Ok(key);
    }

    let width = rect.width().max(1.0);
    let height = rect.height().max(1.0);
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

pub(super) fn overlay_brush_cache_key(
    rect: UiRect,
    style: &OverlayStyle,
) -> D2dOverlayBrushCacheKey {
    D2dOverlayBrushCacheKey {
        rect,
        style_signature: overlay_signature(style),
    }
}

pub(super) fn overlay_signature(style: &OverlayStyle) -> u64 {
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

pub(super) fn radial_gradient_stops(
    layer: lgui::core::RadialGradientLayer,
) -> [D2D1_GRADIENT_STOP; 5] {
    [0.0_f32, 0.25, 0.5, 0.75, 1.0].map(|position| D2D1_GRADIENT_STOP {
        position,
        color: d2d_color_alpha(layer.color, layer.alpha * (1.0 - position).powi(2)),
    })
}

pub(super) fn draw_backdrop_blur(
    resources: &mut D2dRenderer,
    rect: UiRect,
    style: lgui::core::BackdropBlurStyle,
) -> Result<()> {
    let opacity = style.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return Ok(());
    }

    let key = backdrop_blur_cache_key(rect, style);
    if let Some(bitmap) = resources.bitmap_cache.get(&key) {
        draw_bitmap_opacity(&resources.context, rect, &bitmap, opacity);
        return Ok(());
    }

    let Some(result) = with_backdrop_blur_bgra(rect, style, |pixels, width, height, opacity| {
        let bitmap = create_bgra_bitmap(&resources.context, width, height, pixels)?;
        resources.bitmap_cache.insert(key, bitmap.clone());
        draw_bitmap_opacity(&resources.context, rect, &bitmap, opacity);
        Ok(())
    }) else {
        return Ok(());
    };
    result
}

pub(super) fn backdrop_blur_cache_key(
    rect: UiRect,
    style: lgui::core::BackdropBlurStyle,
) -> D2dBitmapCacheKey {
    let (width, height) = raster_size(rect);
    D2dBitmapCacheKey::BackdropBlur {
        signature: backdrop_blur_signature(rect, style),
        width,
        height,
    }
}

pub(super) fn draw_backdrop_blur_path(
    resources: &mut D2dRenderer,
    rect: UiRect,
    path: &UiPath,
    style: lgui::core::BackdropBlurStyle,
) -> Result<()> {
    let opacity = style.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return Ok(());
    }

    let key = backdrop_blur_path_cache_key(rect, path, style);
    if let Some(bitmap) = resources.bitmap_cache.get(&key) {
        draw_bitmap_opacity(&resources.context, rect, &bitmap, opacity);
        return Ok(());
    }

    let Some(result) = with_backdrop_blur_bgra(rect, style, |pixels, width, height, opacity| {
        let mut masked = pixels.to_vec();
        if let Some(points) = polygon_points(path) {
            mask_polygon(&mut masked, width, height, rect, &points);
        }
        let bitmap = create_bgra_bitmap(&resources.context, width, height, &masked)?;
        resources.bitmap_cache.insert(key, bitmap.clone());
        draw_bitmap_opacity(&resources.context, rect, &bitmap, opacity);
        Ok(())
    }) else {
        return Ok(());
    };
    result
}

pub(super) fn create_bgra_bitmap(
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

pub(super) fn draw_bitmap(context: &ID2D1DeviceContext, rect: UiRect, bitmap: &ID2D1Bitmap1) {
    draw_bitmap_opacity(context, rect, bitmap, 1.0);
}

pub(super) fn draw_bitmap_opacity(
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

pub(super) fn draw_compositing_layer_bitmap(
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

pub(super) fn d2d_layer_transform(
    rect: UiRect,
    transform: LayerTransform,
) -> windows_numerics::Matrix3x2 {
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

pub(super) fn polygon_points(path: &UiPath) -> Option<Vec<lgui::core::Point>> {
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

pub(super) fn mask_polygon(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    rect: UiRect,
    points: &[lgui::core::Point],
) {
    for y in 0..height {
        for x in 0..width {
            if !point_in_polygon(rect.left + x as f32, rect.top + y as f32, points) {
                let index = ((y * width + x) * 4) as usize;
                pixels[index..index + 4].fill(0);
            }
        }
    }
}

pub(super) fn point_in_polygon(x: f32, y: f32, points: &[lgui::core::Point]) -> bool {
    let mut inside = false;
    let mut previous = points.len() - 1;
    for current in 0..points.len() {
        let a = points[current];
        let b = points[previous];
        if (a.y > y) != (b.y > y) {
            let intersection_x = (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x;
            if x < intersection_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

pub(super) fn backdrop_blur_signature(rect: UiRect, style: lgui::core::BackdropBlurStyle) -> u64 {
    let mut hasher = DefaultHasher::new();
    "backdrop-blur-bitmap".hash(&mut hasher);
    style.source.hash(&mut hasher);
    style.fit.hash(&mut hasher);
    rect.left.to_bits().hash(&mut hasher);
    rect.top.to_bits().hash(&mut hasher);
    rect.right.to_bits().hash(&mut hasher);
    rect.bottom.to_bits().hash(&mut hasher);
    style.source_rect.left.to_bits().hash(&mut hasher);
    style.source_rect.top.to_bits().hash(&mut hasher);
    style.source_rect.right.to_bits().hash(&mut hasher);
    style.source_rect.bottom.to_bits().hash(&mut hasher);
    style.radius.to_bits().hash(&mut hasher);
    style.tint.0.hash(&mut hasher);
    style.tint_alpha.to_bits().hash(&mut hasher);
    hasher.finish()
}

pub(super) fn backdrop_blur_path_cache_key(
    rect: UiRect,
    path: &UiPath,
    style: lgui::core::BackdropBlurStyle,
) -> D2dBitmapCacheKey {
    let (width, height) = raster_size(rect);
    let mut hasher = DefaultHasher::new();
    "backdrop-blur-path-bitmap".hash(&mut hasher);
    backdrop_blur_signature(rect, style).hash(&mut hasher);
    path.commands().len().hash(&mut hasher);
    for command in path.commands() {
        match command {
            UiPathCommand::MoveTo(point) => {
                "move".hash(&mut hasher);
                hash_point(point, &mut hasher);
            }
            UiPathCommand::LineTo(point) => {
                "line".hash(&mut hasher);
                hash_point(point, &mut hasher);
            }
            UiPathCommand::QuadraticTo { control, to } => {
                "quadratic".hash(&mut hasher);
                hash_point(control, &mut hasher);
                hash_point(to, &mut hasher);
            }
            UiPathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                "cubic".hash(&mut hasher);
                hash_point(control1, &mut hasher);
                hash_point(control2, &mut hasher);
                hash_point(to, &mut hasher);
            }
            UiPathCommand::Close => "close".hash(&mut hasher),
        }
    }
    D2dBitmapCacheKey::BackdropBlurPath {
        signature: hasher.finish(),
        width,
        height,
    }
}

fn hash_point(point: &lgui::core::Point, hasher: &mut DefaultHasher) {
    point.x.to_bits().hash(hasher);
    point.y.to_bits().hash(hasher);
}
