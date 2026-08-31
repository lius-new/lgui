use super::*;

pub(super) fn draw_line(hdc: HDC, start: Point, end: Point, stroke: Stroke) {
    if stroke.alpha == 0 || stroke.width <= 0.0 {
        return;
    }
    unsafe {
        let pen = CreatePen(
            PS_SOLID,
            raster_length(stroke.width),
            colorref(stroke.color),
        );
        let old_pen = SelectObject(hdc, pen.into());
        let _ = MoveToEx(hdc, round_coord(start.x), round_coord(start.y), None);
        let _ = LineTo(hdc, round_coord(end.x), round_coord(end.y));
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
    }
}

pub(super) fn draw_rect(hdc: HDC, rect: UiRect, style: VisualStyle) {
    if style.fill_alpha == 0 && style.stroke.map(|stroke| stroke.alpha).unwrap_or(0) == 0 {
        return;
    }
    let rect = win_rect(rect);
    if draw_antialiased_rect(hdc, rect, style) {
        return;
    }

    unsafe {
        let brush = style
            .fill
            .map(|color| CreateSolidBrush(colorref(color)))
            .unwrap_or(HBRUSH::default());
        let pen = style.stroke.map(|stroke| {
            CreatePen(
                PS_SOLID,
                raster_length(stroke.width),
                colorref(stroke.color),
            )
        });

        if let Some(pen) = pen {
            let old_pen = SelectObject(hdc, pen.into());
            if style.fill.is_some() {
                let old_brush = SelectObject(hdc, brush.into());
                let _ = RoundRect(
                    hdc,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    raster_length(style.radius * 2.0),
                    raster_length(style.radius * 2.0),
                );
                let _ = SelectObject(hdc, old_brush);
            } else {
                let _ = RoundRect(
                    hdc,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    raster_length(style.radius * 2.0),
                    raster_length(style.radius * 2.0),
                );
            }
            let _ = SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen.into());
        } else if style.fill.is_some() {
            let _ = FillRect(hdc, &rect, brush);
        }

        if style.fill.is_some() {
            let _ = DeleteObject(brush.into());
        }
    }
}

pub(super) fn draw_ellipse(hdc: HDC, rect: UiRect, style: VisualStyle) {
    if style.fill_alpha == 0 && style.stroke.map(|stroke| stroke.alpha).unwrap_or(0) == 0 {
        return;
    }
    let rect = win_rect(rect);
    if draw_antialiased_ellipse(hdc, rect, style) {
        return;
    }

    unsafe {
        let fill = style.fill.unwrap_or(Color::BLACK);
        let stroke = style.stroke.unwrap_or(Stroke::new(fill, 1.0, 0));
        let brush = CreateSolidBrush(colorref(fill));
        let pen = CreatePen(
            PS_SOLID,
            raster_length(stroke.width),
            colorref(stroke.color),
        );
        let old_brush = SelectObject(hdc, brush.into());
        let old_pen = SelectObject(hdc, pen.into());
        let _ = Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }
}

pub(super) fn draw_overlay(hdc: HDC, rect: UiRect, style: &OverlayStyle) {
    let start = Instant::now();
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    let key = OverlayCacheKey {
        width,
        height,
        style_signature: overlay_signature(style),
    };
    OVERLAY_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&key) {
            cache.insert(key, rasterize_overlay(width, height, style));
        }
        if let Some(pixels) = cache.get(&key) {
            let gdi_key = overlay_gdi_cache_key(&key);
            if blit_cached_gdi_bitmap(
                GdiFrameBlitSource::Overlay,
                hdc,
                &gdi_key,
                rect,
                UiRect::new(0.0, 0.0, width as f32, height as f32),
                width,
                height,
                pixels,
                255,
            ) {
                return;
            }
            blit_premultiplied_bgra_with_source(
                GdiFrameBlitSource::Overlay,
                hdc,
                rect,
                width,
                height,
                pixels,
            );
        }
    });
    trace_duration("gdi.draw_overlay", start.elapsed());
}

pub(super) fn overlay_gdi_cache_key(key: &OverlayCacheKey) -> String {
    format!(
        "overlay:{}:{}:{:016x}",
        key.width, key.height, key.style_signature
    )
}

pub(super) fn rasterize_overlay(width: i32, height: i32, style: &OverlayStyle) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for layer in &style.vertical_layers {
        composite_vertical_gradient(&mut pixels, width, height, *layer);
    }
    for layer in &style.radial_layers {
        composite_radial_gradient(&mut pixels, width, height, *layer);
    }
    pixels
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

pub(super) fn composite_vertical_gradient(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    layer: VerticalGradientLayer,
) {
    let (red, green, blue) = color_components(layer.color);
    for y in 0..height {
        let t = if height <= 1 {
            1.0
        } else {
            y as f32 / (height - 1) as f32
        };
        let alpha = layer.alpha_top + (layer.alpha_bottom - layer.alpha_top) * t;
        let alpha_u8 = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha_u8 == 0 {
            continue;
        }
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            composite_premultiplied_pixel(
                &mut pixels[index..index + 4],
                red,
                green,
                blue,
                alpha_u8,
            );
        }
    }
}

pub(super) fn composite_radial_gradient(
    pixels: &mut [u8],
    width: i32,
    height: i32,
    layer: RadialGradientLayer,
) {
    let (red, green, blue) = color_components(layer.color);
    let center_x = width as f32 * layer.center_x;
    let center_y = height as f32 * layer.center_y;
    let radius = (width.min(height) as f32 * layer.radius).max(1.0);

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let distance = ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
            let falloff = (1.0 - distance).powf(2.0);
            let alpha = (layer.alpha * falloff).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let index = ((y * width + x) * 4) as usize;
            composite_premultiplied_pixel(
                &mut pixels[index..index + 4],
                red,
                green,
                blue,
                (alpha * 255.0).round() as u8,
            );
        }
    }
}

pub(super) fn composite_premultiplied_pixel(
    pixel: &mut [u8],
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
) {
    let alpha_f = alpha as f32 / 255.0;
    let inv_alpha = 1.0 - alpha_f;
    pixel[0] = ((blue as f32 * alpha_f) + (pixel[0] as f32 * inv_alpha)).round() as u8;
    pixel[1] = ((green as f32 * alpha_f) + (pixel[1] as f32 * inv_alpha)).round() as u8;
    pixel[2] = ((red as f32 * alpha_f) + (pixel[2] as f32 * inv_alpha)).round() as u8;
    pixel[3] = ((alpha as f32) + (pixel[3] as f32 * inv_alpha)).round() as u8;
}

pub(super) fn blit_premultiplied_bgra_with_source(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    rect: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
) {
    blit_premultiplied_bgra_alpha_with_source(source_kind, hdc, rect, width, height, pixels, 255);
}

pub(super) fn blit_premultiplied_bgra_alpha_with_source(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    rect: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) {
    blit_premultiplied_bgra_region_alpha_with_source(
        source_kind,
        hdc,
        rect,
        UiRect::new(0.0, 0.0, width as f32, height as f32),
        width,
        height,
        pixels,
        source_alpha,
    );
}

pub(super) fn blit_cached_gdi_bitmap(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    cache_key: &str,
    dest: UiRect,
    source: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) -> bool {
    if dest.width() <= 0.0
        || dest.height() <= 0.0
        || source.width() <= 0.0
        || source.height() <= 0.0
    {
        return true;
    }
    GDI_BITMAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let Some(entry) = cache.entry(hdc, cache_key, width, height, pixels) else {
            return false;
        };
        let dest_px = pixel_rect_outward(dest);
        let source_px = pixel_rect_outward(source);
        unsafe {
            if source_alpha == 255 && entry.opaque {
                record_gdi_frame_blit(source_kind, GdiFrameBlitKind::BitBlt, dest);
                let _ = BitBlt(
                    hdc,
                    dest_px.left,
                    dest_px.top,
                    dest_px.width(),
                    dest_px.height(),
                    Some(entry.memory_dc),
                    source_px.left,
                    source_px.top,
                    SRCCOPY,
                );
                return true;
            }
            record_gdi_frame_blit(source_kind, GdiFrameBlitKind::AlphaBlend, dest);
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: source_alpha,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = AlphaBlend(
                hdc,
                dest_px.left,
                dest_px.top,
                dest_px.width(),
                dest_px.height(),
                entry.memory_dc,
                source_px.left,
                source_px.top,
                source_px.width(),
                source_px.height(),
                blend,
            );
        }
        true
    })
}

pub(super) fn blit_premultiplied_bgra_region_alpha_with_source(
    source_kind: GdiFrameBlitSource,
    hdc: HDC,
    dest: UiRect,
    source: UiRect,
    width: i32,
    height: i32,
    pixels: &[u8],
    source_alpha: u8,
) {
    let dest_px = pixel_rect_outward(dest);
    let source_px = pixel_rect_outward(source);
    unsafe {
        record_gdi_frame_blit(source_kind, GdiFrameBlitKind::FallbackAlphaBlend, dest);
        let memory_dc = CreateCompatibleDC(Some(hdc));
        if memory_dc.is_invalid() {
            return;
        }

        let mut bits = std::ptr::null_mut();
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let Ok(bitmap) =
            CreateDIBSection(Some(hdc), &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(memory_dc);
            return;
        };
        if bitmap.is_invalid() || bits.is_null() {
            let _ = DeleteDC(memory_dc);
            return;
        }

        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
        let old_bitmap = SelectObject(memory_dc, bitmap.into());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: source_alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = AlphaBlend(
            hdc,
            dest_px.left,
            dest_px.top,
            dest_px.width(),
            dest_px.height(),
            memory_dc,
            source_px.left,
            source_px.top,
            source_px.width(),
            source_px.height(),
            blend,
        );
        let _ = SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_dc);
    }
}

pub(super) fn with_dib_section<T>(
    hdc: HDC,
    width: i32,
    height: i32,
    draw: impl FnOnce(HDC, *mut std::ffi::c_void) -> T,
) -> Option<T> {
    unsafe {
        let memory_dc = CreateCompatibleDC(Some(hdc));
        if memory_dc.is_invalid() {
            return None;
        }

        let mut bits = std::ptr::null_mut();
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let Ok(bitmap) =
            CreateDIBSection(Some(hdc), &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(memory_dc);
            return None;
        };
        if bitmap.is_invalid() || bits.is_null() {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            return None;
        }

        let old_bitmap = SelectObject(memory_dc, bitmap.into());
        let result = draw(memory_dc, bits);
        let _ = SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_dc);
        Some(result)
    }
}

pub(super) fn clear_alpha_buffer(bits: *mut std::ffi::c_void, width: i32, height: i32) {
    if bits.is_null() {
        return;
    }
    let len = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), len) };
    pixels.fill(0);
}

pub(super) fn prepare_alpha_buffer(
    bits: *mut std::ffi::c_void,
    width: i32,
    height: i32,
    background: lgui::core::StaticLayerBackground,
) {
    match background {
        lgui::core::StaticLayerBackground::Opaque => set_opaque_alpha(bits, width, height),
        lgui::core::StaticLayerBackground::Transparent => {
            set_drawn_pixel_alpha(bits, width, height)
        }
    }
}

pub(super) fn set_opaque_alpha(bits: *mut std::ffi::c_void, width: i32, height: i32) {
    if bits.is_null() {
        return;
    }
    let len = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), len) };
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

pub(super) fn set_drawn_pixel_alpha(bits: *mut std::ffi::c_void, width: i32, height: i32) {
    if bits.is_null() {
        return;
    }
    let len = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), len) };
    for pixel in pixels.chunks_exact_mut(4) {
        if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
            pixel[3] = 255;
        }
    }
}

pub(super) fn color_components(color: Color) -> (u8, u8, u8) {
    (
        ((color.0 >> 16) & 0xFF) as u8,
        ((color.0 >> 8) & 0xFF) as u8,
        (color.0 & 0xFF) as u8,
    )
}

pub(super) fn trace_duration(label: &str, duration: Duration) {
    if trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}
