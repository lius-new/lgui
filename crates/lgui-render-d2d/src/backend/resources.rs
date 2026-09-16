use super::*;

pub(super) fn draw_text(
    context: &ID2D1DeviceContext,
    dwrite_factory: &IDWriteFactory,
    rect: UiRect,
    text: &str,
    style: lgui_core::core::TextStyle,
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
            style.height.abs(),
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
            rect.width().max(1.0),
            rect.height().max(1.0),
        )?;
        if style.tracking != 0.0 {
            if let Ok(layout1) = layout.cast::<IDWriteTextLayout1>() {
                layout1.SetCharacterSpacing(
                    0.0,
                    style.tracking,
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
        let x = rect.left;
        let y = rect.top + ((rect.height() - metrics.height).max(0.0) / 2.0);
        context.DrawTextLayout(
            windows_numerics::Vector2 { X: x, Y: y },
            &layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
    }
    Ok(())
}

pub(super) fn solid_brush(
    context: &ID2D1DeviceContext,
    color: Color,
    alpha: u8,
) -> Result<ID2D1SolidColorBrush> {
    unsafe { context.CreateSolidColorBrush(&d2d_color(color, alpha), None) }
}

pub(super) fn d2d_rect(rect: UiRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

pub(super) fn vector2(x: f32, y: f32) -> windows_numerics::Vector2 {
    windows_numerics::Vector2 { X: x, Y: y }
}

pub(super) fn rounded_rect(rect: UiRect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: d2d_rect(rect),
        radiusX: radius,
        radiusY: radius,
    }
}

pub(super) fn d2d_color(color: Color, alpha: u8) -> D2D1_COLOR_F {
    d2d_color_alpha(color, alpha as f32 / 255.0)
}

pub(super) fn d2d_color_alpha(color: Color, alpha: f32) -> D2D1_COLOR_F {
    let rgb = color.0;
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: alpha.clamp(0.0, 1.0),
    }
}

pub(super) fn transparent() -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

pub(super) fn opaque_black() -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
}

pub(super) fn static_layer_clear_color(spec: &StaticLayerSpec) -> D2D1_COLOR_F {
    match spec.background {
        StaticLayerBackground::Opaque => opaque_black(),
        StaticLayerBackground::Transparent => transparent(),
    }
}

pub(super) fn create_scene_bitmap(
    context: &ID2D1DeviceContext,
    width: i32,
    height: i32,
) -> Result<ID2D1Bitmap1> {
    create_bitmap_with_options(context, width, height, D2D1_BITMAP_OPTIONS_TARGET, None)
}

pub(super) fn create_bitmap_with_options(
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

pub(super) fn read_bitmap_bgra(
    context: &ID2D1DeviceContext,
    bitmap: &ID2D1Bitmap1,
    width: i32,
    height: i32,
) -> Result<Vec<u8>> {
    use windows::Win32::Graphics::Direct2D::{
        D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_CPU_READ, D2D1_MAP_OPTIONS_READ,
    };
    let staging = create_bitmap_with_options(
        context,
        width,
        height,
        D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        None,
    )?;
    let row_bytes = width as usize * 4;
    let mut pixels = vec![0; row_bytes * height as usize];
    unsafe {
        staging.CopyFromBitmap(None, bitmap, None)?;
        let mapped = staging.Map(D2D1_MAP_OPTIONS_READ)?;
        for y in 0..height as usize {
            let row =
                std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), row_bytes);
            pixels[y * row_bytes..][..row_bytes].copy_from_slice(row);
        }
        staging.Unmap()?;
    }
    Ok(pixels)
}

pub(super) fn static_layer_cache_key(
    id: &UiId,
    spec: &StaticLayerSpec,
    width: i32,
    height: i32,
    child_signature: u64,
) -> D2dBitmapCacheKey {
    D2dBitmapCacheKey::StaticLayer {
        raster_key: super::super::static_layer::static_layer_cache_key(
            id,
            spec,
            width,
            height,
            child_signature,
        ),
        id: id.clone(),
        spec_signature: static_layer_spec_signature(spec),
        child_signature,
        width,
        height,
    }
}

pub(super) fn static_layer_spec_signature(spec: &StaticLayerSpec) -> u64 {
    let mut hasher = DefaultHasher::new();
    spec.cache_signature().hash(&mut hasher);
    hasher.finish()
}

pub(super) fn trace_d2d_regions(label: &str, rects: Option<&[UiRect]>) {
    if !trace::enabled(TraceCategory::RegionDetail) {
        return;
    }
    match rects {
        Some(rects) => {
            let area: i64 = rects
                .iter()
                .map(|rect| rect.width().max(0.0) as i64 * rect.height().max(0.0) as i64)
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

pub(super) fn trace_duration(label: &str, duration: Duration) {
    if trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}
