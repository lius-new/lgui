use super::*;

pub(super) const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;

pub(super) fn draw_gdi_transformed_bitmap(
    hdc: HDC,
    rect: UiRect,
    clip: Option<UiRect>,
    bitmap: &GdiBitmapEntry,
    opacity: u8,
    transform: LayerTransform,
) {
    let _clip = ClipGuard::new(hdc, clip);
    let points = gdi_layer_destination_points(rect, transform);
    unsafe {
        let mut image: *mut GpBitmap = std::ptr::null_mut();
        if GdipCreateBitmapFromScan0(
            bitmap.width,
            bitmap.height,
            bitmap.width * 4,
            PIXEL_FORMAT_32BPP_PARGB,
            Some(bitmap.bits.cast_const()),
            &mut image,
        ) != GpOk
            || image.is_null()
        {
            return;
        }

        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            let _ = GdipDisposeImage(image.cast());
            return;
        }
        let _ = GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let mut attributes: *mut GpImageAttributes = std::ptr::null_mut();
        let attributes_ptr = if opacity == 255 {
            std::ptr::null()
        } else if GdipCreateImageAttributes(&mut attributes) == GpOk && !attributes.is_null() {
            let alpha = opacity as f32 / 255.0;
            let matrix = ColorMatrix {
                m: [
                    1.0, 0.0, 0.0, 0.0, 0.0, // red
                    0.0, 1.0, 0.0, 0.0, 0.0, // green
                    0.0, 0.0, 1.0, 0.0, 0.0, // blue
                    0.0, 0.0, 0.0, alpha, 0.0, // alpha
                    0.0, 0.0, 0.0, 0.0, 1.0,
                ],
            };
            let _ = GdipSetImageAttributesColorMatrix(
                attributes,
                ColorAdjustTypeBitmap,
                true,
                &matrix,
                std::ptr::null(),
                ColorMatrixFlagsDefault,
            );
            attributes.cast_const()
        } else {
            std::ptr::null()
        };

        let _ = GdipDrawImagePointsRect(
            graphics,
            image.cast(),
            points.as_ptr(),
            points.len() as i32,
            0.0,
            0.0,
            bitmap.width as f32,
            bitmap.height as f32,
            UnitPixel,
            attributes_ptr,
            0,
            std::ptr::null_mut(),
        );

        if !attributes.is_null() {
            let _ = GdipDisposeImageAttributes(attributes);
        }
        let _ = GdipDeleteGraphics(graphics);
        let _ = GdipDisposeImage(image.cast());
    }
}

pub(super) fn gdi_layer_destination_points(rect: UiRect, transform: LayerTransform) -> [PointF; 3] {
    let transform_point = |x, y| {
        let (x, y) = transform.transform_point(rect, x, y);
        PointF { X: x, Y: y }
    };
    [
        transform_point(rect.left as f32, rect.top as f32),
        transform_point(rect.right as f32, rect.top as f32),
        transform_point(rect.left as f32, rect.bottom as f32),
    ]
}

pub(super) fn draw_gdi_compositing_layer_fallback(
    hdc: HDC,
    rect: UiRect,
    clip: Option<UiRect>,
    commands: &[ScenePrimitive],
) {
    let saved = unsafe { SaveDC(hdc) };
    if saved == 0 {
        return;
    }
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::SetViewportOrgEx(
            hdc,
            round_coord(rect.left),
            round_coord(rect.top),
            None,
        );
    }
    let local_clip = clip
        .and_then(|clip| rect.intersect(clip))
        .map(|clip| clip.translate(-rect.left, -rect.top));
    for command in commands {
        if local_clip.is_none_or(|clip| ClipRegion::new(clip).intersects(command)) {
            GdiRenderer::draw_command_clipped(hdc, command, local_clip);
        }
    }
    unsafe {
        let _ = RestoreDC(hdc, saved);
    }
}

pub(super) fn draw_backdrop_blur(
    hdc: HDC,
    rect: UiRect,
    style: lgui_core::core::BackdropBlurStyle,
    clip: Option<UiRect>,
) {
    draw_cached_backdrop_blur(hdc, rect, style, clip);
}

pub(super) fn draw_backdrop_blur_path(
    hdc: HDC,
    rect: UiRect,
    path: &UiPath,
    style: lgui_core::core::BackdropBlurStyle,
    clip: Option<UiRect>,
) {
    let path_clip = PolygonClipGuard::new(hdc, path, clip);
    let fallback_clip = if path_clip.is_some() { None } else { clip };
    draw_cached_backdrop_blur(hdc, rect, style, fallback_clip);
}

fn draw_cached_backdrop_blur(
    hdc: HDC,
    rect: UiRect,
    style: lgui_core::core::BackdropBlurStyle,
    clip: Option<UiRect>,
) {
    let source_alpha = (style.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    if source_alpha == 0 {
        return;
    }

    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    let Some((dest, source)) = backdrop_blit_region(rect, clip, width, height) else {
        return;
    };
    let key = backdrop_gdi_cache_key(rect, style);
    if blit_existing_gdi_bitmap(
        GdiFrameBlitSource::Backdrop,
        hdc,
        &key,
        dest,
        source,
        (width, height),
        source_alpha,
    ) {
        return;
    }

    let _ = with_backdrop_blur_bgra(rect, style, |pixels, width, height, _| {
        if blit_cached_gdi_bitmap(
            GdiFrameBlitSource::Backdrop,
            hdc,
            &key,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        ) {
            return;
        }
        blit_premultiplied_bgra_region_alpha_with_source(
            GdiFrameBlitSource::Backdrop,
            hdc,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        );
    });
}

fn backdrop_blit_region(
    rect: UiRect,
    clip: Option<UiRect>,
    width: i32,
    height: i32,
) -> Option<(UiRect, UiRect)> {
    let dest = clip.map_or(Some(rect), |clip| rect.intersect(clip))?;
    let source = if clip.is_some() {
        UiRect::new(
            dest.left - rect.left,
            dest.top - rect.top,
            dest.right - rect.left,
            dest.bottom - rect.top,
        )
    } else {
        UiRect::new(0.0, 0.0, width as f32, height as f32)
    };
    Some((dest, source))
}

pub(super) fn backdrop_gdi_cache_key(
    rect: UiRect,
    style: lgui_core::core::BackdropBlurStyle,
) -> String {
    let mut hasher = DefaultHasher::new();
    "backdrop-blur-gdi".hash(&mut hasher);
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
    format!("backdrop:{:016x}", hasher.finish())
}

pub(super) struct ClipGuard {
    hdc: HDC,
    state: Option<i32>,
}

impl ClipGuard {
    pub(super) fn new(hdc: HDC, clip: Option<UiRect>) -> Self {
        let Some(clip) = clip else {
            return Self { hdc, state: None };
        };
        unsafe {
            let state = SaveDC(hdc);
            if state == 0 {
                return Self { hdc, state: None };
            }
            let clip = win_rect(clip);
            let region = CreateRectRgn(clip.left, clip.top, clip.right, clip.bottom);
            if region.is_invalid() {
                let _ = RestoreDC(hdc, state);
                return Self { hdc, state: None };
            }
            let _ = SelectClipRgn(hdc, Some(region));
            let _ = DeleteObject(region.into());
            Self {
                hdc,
                state: Some(state),
            }
        }
    }
}

impl Drop for ClipGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state {
            unsafe {
                let _ = RestoreDC(self.hdc, state);
            }
        }
    }
}

pub(super) struct PolygonClipGuard {
    hdc: HDC,
    state: i32,
}

impl PolygonClipGuard {
    pub(super) fn new(hdc: HDC, path: &UiPath, clip: Option<UiRect>) -> Option<Self> {
        let points = polygon_points(path)?;
        if points.len() < 3 {
            return None;
        }

        unsafe {
            let state = SaveDC(hdc);
            if state == 0 {
                return None;
            }

            let polygon_region = CreatePolygonRgn(&points, WINDING);
            if polygon_region.is_invalid() {
                let _ = RestoreDC(hdc, state);
                return None;
            }

            if let Some(clip) = clip {
                let clip = win_rect(clip);
                let clip_region = CreateRectRgn(clip.left, clip.top, clip.right, clip.bottom);
                if clip_region.is_invalid() {
                    let _ = DeleteObject(polygon_region.into());
                    let _ = RestoreDC(hdc, state);
                    return None;
                }
                let _ = CombineRgn(
                    Some(polygon_region),
                    Some(polygon_region),
                    Some(clip_region),
                    RGN_AND,
                );
                let _ = DeleteObject(clip_region.into());
            }

            let _ = SelectClipRgn(hdc, Some(polygon_region));
            let _ = DeleteObject(polygon_region.into());
            Some(Self { hdc, state })
        }
    }
}

impl Drop for PolygonClipGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = RestoreDC(self.hdc, self.state);
        }
    }
}

pub(super) fn polygon_points(path: &UiPath) -> Option<Vec<POINT>> {
    let mut points = Vec::new();
    let mut has_close = false;
    for command in path.commands() {
        match *command {
            UiPathCommand::MoveTo(point) | UiPathCommand::LineTo(point) => {
                points.push(POINT {
                    x: round_coord(point.x),
                    y: round_coord(point.y),
                });
            }
            UiPathCommand::Close => {
                has_close = true;
            }
            UiPathCommand::QuadraticTo { .. } | UiPathCommand::CubicTo { .. } => return None,
        }
    }
    if has_close {
        Some(points)
    } else {
        None
    }
}

pub(super) struct GdiStaticLayerBackend;

impl StaticLayerDrawBackend for GdiStaticLayerBackend {
    fn draw_command(hdc: HDC, command: &ScenePrimitive) {
        GdiRenderer::draw_command(hdc, command);
    }

    fn blit_premultiplied_bgra_alpha(
        hdc: HDC,
        rect: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    ) {
        blit_premultiplied_bgra_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            rect,
            width,
            height,
            pixels,
            source_alpha,
        );
    }

    fn blit_premultiplied_bgra_region_alpha(
        hdc: HDC,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    ) {
        blit_premultiplied_bgra_region_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        );
    }

    fn blit_cached_premultiplied_bgra_region_alpha(
        hdc: HDC,
        cache_key: &str,
        dest: UiRect,
        source: UiRect,
        width: i32,
        height: i32,
        pixels: &[u8],
        source_alpha: u8,
    ) {
        if blit_cached_gdi_bitmap(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            cache_key,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        ) {
            return;
        }
        blit_premultiplied_bgra_region_alpha_with_source(
            GdiFrameBlitSource::StaticLayer,
            hdc,
            dest,
            source,
            width,
            height,
            pixels,
            source_alpha,
        );
    }

    fn with_dib_section<T>(
        hdc: HDC,
        width: i32,
        height: i32,
        draw: impl FnOnce(HDC, *mut std::ffi::c_void) -> T,
    ) -> Option<T> {
        with_dib_section(hdc, width, height, draw)
    }

    fn clear_alpha_buffer(bits: *mut std::ffi::c_void, width: i32, height: i32) {
        clear_alpha_buffer(bits, width, height);
    }

    fn prepare_alpha_buffer(
        bits: *mut std::ffi::c_void,
        width: i32,
        height: i32,
        background: lgui_core::core::StaticLayerBackground,
    ) {
        prepare_alpha_buffer(bits, width, height, background);
    }
}
