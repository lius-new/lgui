fn draw_text(hdc: HDC, rect: UiRect, text: &str, style: TextStyle) {
    if style.alpha == 0 || text.is_empty() {
        return;
    }
    unsafe {
        let height = round_coord(style.height);
        let tracking = round_coord(style.tracking);
        let runs = gdi_text_runs(hdc, text, height, style.weight);
        let previous_extra = SetTextCharacterExtra(hdc, tracking);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, colorref(style.color));

        if runs.len() <= 1 {
            let family_index = runs.first().map(|run| run.family_index).unwrap_or(0);
            if let Some(font) = create_gdi_font(height, style.weight, family_index) {
                let old_font = SelectObject(hdc, font.into());
                let align = match style.align {
                    TextAlign::Left => DT_LEFT,
                    TextAlign::Center => DT_CENTER,
                    TextAlign::Right => DT_RIGHT,
                };
                let mut draw_rect = win_rect(rect);
                let mut wide: Vec<u16> = text.encode_utf16().collect();
                let _ = DrawTextW(
                    hdc,
                    &mut wide,
                    &mut draw_rect,
                    DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | align,
                );
                let _ = SelectObject(hdc, old_font);
                let _ = DeleteObject(font.into());
            }
        } else {
            let total_width = gdi_text_runs_width(hdc, &runs, height, style.weight, tracking);
            let mut left = match style.align {
                TextAlign::Left => rect.left,
                TextAlign::Center => {
                    rect.left + ((rect.width() - total_width as f32).max(0.0) / 2.0)
                }
                TextAlign::Right => rect.right - total_width as f32,
            };
            for run in &runs {
                if let Some(font) = create_gdi_font(height, style.weight, run.family_index) {
                    let old_font = SelectObject(hdc, font.into());
                    let run_width = gdi_text_width(hdc, &run.text, tracking).unwrap_or(0);
                    let mut run_rect = RECT {
                        left: round_coord(left),
                        top: round_coord(rect.top),
                        right: round_coord(rect.right),
                        bottom: round_coord(rect.bottom),
                    };
                    let mut wide: Vec<u16> = run.text.encode_utf16().collect();
                    let _ = DrawTextW(
                        hdc,
                        &mut wide,
                        &mut run_rect,
                        DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_LEFT,
                    );
                    left += run_width as f32;
                    let _ = SelectObject(hdc, old_font);
                    let _ = DeleteObject(font.into());
                }
            }
        };

        let _ = SetTextCharacterExtra(hdc, previous_extra);
    }
}

#[derive(Clone)]
struct GdiTextRun {
    family_index: usize,
    text: String,
}

fn gdi_text_runs(hdc: HDC, text: &str, height: i32, weight: i32) -> Vec<GdiTextRun> {
    let mut runs: Vec<GdiTextRun> = Vec::new();
    for ch in text.chars() {
        let family_index = gdi_font_family_for_char(hdc, ch, height, weight);
        if let Some(run) = runs.last_mut() {
            if run.family_index == family_index {
                run.text.push(ch);
                continue;
            }
        }
        runs.push(GdiTextRun {
            family_index,
            text: ch.to_string(),
        });
    }
    runs
}

fn gdi_font_family_for_char(hdc: HDC, ch: char, height: i32, weight: i32) -> usize {
    GDI_FONT_FAMILY_CACHE.with(|cache| {
        let key = (ch, height, weight);
        if let Some(family_index) = cache.borrow().get(&key).copied() {
            return family_index;
        }
        let family_index = unsafe {
            (0..ui_font_family_count())
                .find(|family_index| {
                    gdi_font_family_supports_char(hdc, ch, height, weight, *family_index)
                })
                .unwrap_or(0)
        };
        cache.borrow_mut().insert(key, family_index);
        family_index
    })
}

unsafe fn gdi_font_family_supports_char(
    hdc: HDC,
    ch: char,
    height: i32,
    weight: i32,
    family_index: usize,
) -> bool {
    let Some(font) = create_gdi_font(height, weight, family_index) else {
        return false;
    };
    let old_font = SelectObject(hdc, font.into());
    let mut utf16 = [0u16; 2];
    let units = ch.encode_utf16(&mut utf16);
    let mut glyphs = vec![0u16; units.len()];
    let result = GetGlyphIndicesW(
        hdc,
        PCWSTR(units.as_ptr()),
        units.len() as i32,
        glyphs.as_mut_ptr(),
        GGI_MARK_NONEXISTING_GLYPHS,
    );
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font.into());
    result != u32::MAX && glyphs.iter().all(|glyph| *glyph != 0xFFFF)
}

unsafe fn create_gdi_font(height: i32, weight: i32, family_index: usize) -> Option<HFONT> {
    let font = CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_TT_ONLY_PRECIS,
        windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        ui_font_family_at(family_index),
    );
    (!font.is_invalid()).then_some(font)
}

unsafe fn gdi_text_runs_width(
    hdc: HDC,
    runs: &[GdiTextRun],
    height: i32,
    weight: i32,
    tracking: i32,
) -> i32 {
    runs.iter()
        .filter_map(|run| {
            let font = create_gdi_font(height, weight, run.family_index)?;
            let old_font = SelectObject(hdc, font.into());
            let width = gdi_text_width(hdc, &run.text, tracking);
            let _ = SelectObject(hdc, old_font);
            let _ = DeleteObject(font.into());
            width
        })
        .sum()
}

unsafe fn gdi_text_width(hdc: HDC, text: &str, tracking: i32) -> Option<i32> {
    if text.is_empty() {
        return Some(0);
    }
    let previous_extra = SetTextCharacterExtra(hdc, tracking);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    let measured = GetTextExtentPoint32W(hdc, &wide, &mut size).as_bool();
    let _ = SetTextCharacterExtra(hdc, previous_extra);
    measured.then_some(size.cx)
}

fn colorref(color: Color) -> windows::Win32::Foundation::COLORREF {
    let value = color.0;
    windows::Win32::Foundation::COLORREF(
        ((value & 0xFF) << 16) | (value & 0x00FF00) | ((value >> 16) & 0xFF),
    )
}

fn win_rect(rect: UiRect) -> RECT {
    let rect = pixel_rect_outward(rect);
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn draw_antialiased_rect(hdc: HDC, rect: RECT, style: VisualStyle) -> bool {
    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return false;
        }

        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let path = create_rect_path(rect, round_coord(style.radius));
        if path.is_null() {
            let _ = GdipDeleteGraphics(graphics);
            return false;
        }

        let ok = fill_and_stroke_path(graphics, path, style);
        let _ = GdipDeletePath(path);
        let _ = GdipDeleteGraphics(graphics);
        ok
    }
}

fn draw_antialiased_ellipse(hdc: HDC, rect: RECT, style: VisualStyle) -> bool {
    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return false;
        }

        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        if let Some(fill) = style.fill {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color_to_argb(fill, style.fill_alpha), &mut brush) != GpOk
                || brush.is_null()
            {
                let _ = GdipDeleteGraphics(graphics);
                return false;
            }
            let _ = GdipFillEllipseI(
                graphics,
                brush as *mut GpBrush,
                rect.left,
                rect.top,
                width,
                height,
            );
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }

        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0.0 {
                let mut pen: *mut GpPen = std::ptr::null_mut();
                if GdipCreatePen1(
                    color_to_argb(stroke.color, stroke.alpha),
                    stroke.width,
                    UnitPixel,
                    &mut pen,
                ) != GpOk
                    || pen.is_null()
                {
                    let _ = GdipDeleteGraphics(graphics);
                    return false;
                }
                let inset = round_coord(stroke.width / 2.0);
                let stroke_width = raster_length(stroke.width);
                let _ = GdipDrawEllipseI(
                    graphics,
                    pen,
                    rect.left + inset,
                    rect.top + inset,
                    (width - stroke_width).max(1),
                    (height - stroke_width).max(1),
                );
                let _ = GdipDeletePen(pen);
            }
        }

        let _ = GdipDeleteGraphics(graphics);
        true
    }
}

fn fill_and_stroke_path(graphics: *mut GpGraphics, path: *mut GpPath, style: VisualStyle) -> bool {
    unsafe {
        if let Some(fill) = style.fill {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color_to_argb(fill, style.fill_alpha), &mut brush) != GpOk
                || brush.is_null()
            {
                return false;
            }
            let _ = GdipFillPath(graphics, brush as *mut GpBrush, path);
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }

        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0.0 {
                let mut pen: *mut GpPen = std::ptr::null_mut();
                if GdipCreatePen1(
                    color_to_argb(stroke.color, stroke.alpha),
                    stroke.width,
                    UnitPixel,
                    &mut pen,
                ) != GpOk
                    || pen.is_null()
                {
                    return false;
                }
                let _ = GdipDrawPath(graphics, pen, path);
                let _ = GdipDeletePen(pen);
            }
        }
        true
    }
}

fn fill_and_stroke_ui_path(graphics: *mut GpGraphics, path: *mut GpPath, style: PathStyle) -> bool {
    unsafe {
        if let Some(fill) = style.fill {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(color_to_argb(fill, style.fill_alpha), &mut brush) != GpOk
                || brush.is_null()
            {
                return false;
            }
            let _ = GdipFillPath(graphics, brush as *mut GpBrush, path);
            let _ = GdipDeleteBrush(brush as *mut GpBrush);
        }

        if let Some(stroke) = style.stroke {
            if stroke.alpha != 0 && stroke.width > 0.0 {
                let mut pen: *mut GpPen = std::ptr::null_mut();
                if GdipCreatePen1(
                    color_to_argb(stroke.color, stroke.alpha),
                    stroke.width,
                    UnitPixel,
                    &mut pen,
                ) != GpOk
                    || pen.is_null()
                {
                    return false;
                }
                let _ = GdipDrawPath(graphics, pen, path);
                let _ = GdipDeletePen(pen);
            }
        }
        true
    }
}

fn draw_path(hdc: HDC, path: &UiPath, style: PathStyle) {
    if path.commands().is_empty() {
        return;
    }

    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return;
        }
        let _ = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let gp_path = create_ui_path(path);
        if !gp_path.is_null() {
            let _ = fill_and_stroke_ui_path(graphics, gp_path, style);
            let _ = GdipDeletePath(gp_path);
        }
        let _ = GdipDeleteGraphics(graphics);
    }
}

fn create_ui_path(path: &UiPath) -> *mut GpPath {
    unsafe {
        let mut gp_path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut gp_path) != GpOk || gp_path.is_null() {
            return std::ptr::null_mut();
        }

        let mut current: Option<Point> = None;
        let mut figure_start: Option<Point> = None;
        for command in path.commands() {
            let status = match *command {
                UiPathCommand::MoveTo(point) => {
                    current = Some(point);
                    figure_start = Some(point);
                    GpOk
                }
                UiPathCommand::LineTo(point) => {
                    let Some(from) = current else {
                        current = Some(point);
                        figure_start = Some(point);
                        continue;
                    };
                    current = Some(point);
                    GdipAddPathLineI(
                        gp_path,
                        round_coord(from.x),
                        round_coord(from.y),
                        round_coord(point.x),
                        round_coord(point.y),
                    )
                }
                UiPathCommand::QuadraticTo { control, to } => {
                    let Some(from) = current else {
                        current = Some(to);
                        figure_start = Some(to);
                        continue;
                    };
                    let control1 = Point::new(
                        from.x + ((control.x - from.x) * 2.0) / 3.0,
                        from.y + ((control.y - from.y) * 2.0) / 3.0,
                    );
                    let control2 = Point::new(
                        to.x + ((control.x - to.x) * 2.0) / 3.0,
                        to.y + ((control.y - to.y) * 2.0) / 3.0,
                    );
                    current = Some(to);
                    GdipAddPathBezierI(
                        gp_path,
                        round_coord(from.x),
                        round_coord(from.y),
                        round_coord(control1.x),
                        round_coord(control1.y),
                        round_coord(control2.x),
                        round_coord(control2.y),
                        round_coord(to.x),
                        round_coord(to.y),
                    )
                }
                UiPathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    let Some(from) = current else {
                        current = Some(to);
                        figure_start = Some(to);
                        continue;
                    };
                    current = Some(to);
                    GdipAddPathBezierI(
                        gp_path,
                        round_coord(from.x),
                        round_coord(from.y),
                        round_coord(control1.x),
                        round_coord(control1.y),
                        round_coord(control2.x),
                        round_coord(control2.y),
                        round_coord(to.x),
                        round_coord(to.y),
                    )
                }
                UiPathCommand::Close => {
                    current = figure_start;
                    GdipClosePathFigure(gp_path)
                }
            };
            if status != GpOk {
                let _ = GdipDeletePath(gp_path);
                return std::ptr::null_mut();
            }
        }

        gp_path
    }
}

fn create_rect_path(rect: RECT, radius: i32) -> *mut GpPath {
    if radius > 0 {
        return create_rounded_rect_path(rect, radius);
    }

    unsafe {
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != GpOk || path.is_null() {
            return std::ptr::null_mut();
        }

        let statuses = [
            GdipAddPathLineI(path, rect.left, rect.top, rect.right, rect.top),
            GdipAddPathLineI(path, rect.right, rect.top, rect.right, rect.bottom),
            GdipAddPathLineI(path, rect.right, rect.bottom, rect.left, rect.bottom),
            GdipAddPathLineI(path, rect.left, rect.bottom, rect.left, rect.top),
            GdipClosePathFigure(path),
        ];
        if statuses.iter().any(|status| *status != GpOk) {
            let _ = GdipDeletePath(path);
            return std::ptr::null_mut();
        }
        path
    }
}

fn create_rounded_rect_path(rect: RECT, radius: i32) -> *mut GpPath {
    unsafe {
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) != GpOk || path.is_null() {
            return std::ptr::null_mut();
        }

        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        let diameter = (radius * 2).min(width).min(height).max(2);
        let line_radius = diameter / 2;
        let right = rect.right - diameter;
        let bottom = rect.bottom - diameter;

        let statuses = [
            GdipAddPathArcI(path, rect.left, rect.top, diameter, diameter, 180.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.left + line_radius,
                rect.top,
                rect.right - line_radius,
                rect.top,
            ),
            GdipAddPathArcI(path, right, rect.top, diameter, diameter, 270.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.right,
                rect.top + line_radius,
                rect.right,
                rect.bottom - line_radius,
            ),
            GdipAddPathArcI(path, right, bottom, diameter, diameter, 0.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.right - line_radius,
                rect.bottom,
                rect.left + line_radius,
                rect.bottom,
            ),
            GdipAddPathArcI(path, rect.left, bottom, diameter, diameter, 90.0, 90.0),
            GdipAddPathLineI(
                path,
                rect.left,
                rect.bottom - line_radius,
                rect.left,
                rect.top + line_radius,
            ),
            GdipClosePathFigure(path),
        ];

        if statuses.iter().any(|status| *status != GpOk) {
            let _ = GdipDeletePath(path);
            return std::ptr::null_mut();
        }

        path
    }
}

fn color_to_argb(color: Color, alpha: u8) -> u32 {
    let red = (color.0 >> 16) & 0xFF;
    let green = (color.0 >> 8) & 0xFF;
    let blue = color.0 & 0xFF;
    ((alpha as u32) << 24) | (red << 16) | (green << 8) | blue
}
