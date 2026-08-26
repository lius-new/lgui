use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{COLORREF, RECT},
        Graphics::Gdi::{
            CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, FillRect,
            IntersectClipRect, LineTo, MoveToEx, RestoreDC, RoundRect, SaveDC, SelectObject,
            SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
            DEFAULT_QUALITY, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
            FF_DONTCARE, HDC, HGDIOBJ, OUT_TT_ONLY_PRECIS, PS_SOLID, TRANSPARENT,
        },
    },
};

use crate::{
    core::{Color, Scene, ScenePrimitive, TextAlign, TextStyle, UiRect, VisualStyle},
    renderer::RenderBackend,
};

pub struct GdiRenderer {
    background: Color,
}

impl GdiRenderer {
    pub fn new(background: Color) -> Self {
        Self { background }
    }

    pub fn clear(&self, target: HDC, rect: UiRect) {
        fill_rect(target, rect, self.background);
    }

    fn draw_commands(&self, target: HDC, commands: &[ScenePrimitive]) {
        for command in commands {
            self.draw_command(target, command);
        }
    }

    fn draw_command(&self, target: HDC, command: &ScenePrimitive) {
        match command {
            ScenePrimitive::Rect { rect, style, .. } => draw_rect(target, *rect, *style),
            ScenePrimitive::Ellipse { rect, style, .. } => draw_ellipse(target, *rect, *style),
            ScenePrimitive::Text {
                rect, text, style, ..
            } => draw_text(target, *rect, text, *style),
            ScenePrimitive::Line {
                start, end, stroke, ..
            } => draw_line(target, *start, *end, *stroke),
            ScenePrimitive::Clip { rect, commands, .. }
            | ScenePrimitive::ClipPath { rect, commands, .. } => {
                let saved = unsafe { SaveDC(target) };
                if saved != 0 {
                    unsafe {
                        let _ =
                            IntersectClipRect(target, rect.left, rect.top, rect.right, rect.bottom);
                    }
                    self.draw_commands(target, commands);
                    unsafe {
                        let _ = RestoreDC(target, saved);
                    }
                }
            }
            ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. } => {
                self.draw_commands(target, commands);
            }
            ScenePrimitive::Custom { .. }
            | ScenePrimitive::Path { .. }
            | ScenePrimitive::Image { .. }
            | ScenePrimitive::Icon { .. }
            | ScenePrimitive::Glow { .. }
            | ScenePrimitive::BackdropBlur { .. }
            | ScenePrimitive::BackdropBlurPath { .. }
            | ScenePrimitive::Overlay { .. } => {}
        }
    }
}

impl Default for GdiRenderer {
    fn default() -> Self {
        Self::new(Color(0x111418))
    }
}

impl RenderBackend<HDC> for GdiRenderer {
    fn draw_scene(&mut self, target: HDC, scene: &Scene, clip: Option<UiRect>) {
        let saved = clip.map(|clip| unsafe {
            let saved = SaveDC(target);
            if saved != 0 {
                let _ = IntersectClipRect(target, clip.left, clip.top, clip.right, clip.bottom);
            }
            saved
        });
        self.draw_commands(target, scene.commands());
        if let Some(saved) = saved.filter(|saved| *saved != 0) {
            unsafe {
                let _ = RestoreDC(target, saved);
            }
        }
    }
}

fn draw_rect(target: HDC, rect: UiRect, style: VisualStyle) {
    if let Some(fill) = style.fill {
        if style.radius > 0 {
            let brush = unsafe { CreateSolidBrush(colorref(fill)) };
            let previous = unsafe { SelectObject(target, HGDIOBJ(brush.0)) };
            unsafe {
                let _ = RoundRect(
                    target,
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.bottom,
                    style.radius * 2,
                    style.radius * 2,
                );
                let _ = SelectObject(target, previous);
                let _ = DeleteObject(brush.into());
            }
        } else {
            fill_rect(target, rect, fill);
        }
    }
    if let Some(stroke) = style.stroke {
        let pen = unsafe { CreatePen(PS_SOLID, stroke.width.max(1), colorref(stroke.color)) };
        let previous = unsafe { SelectObject(target, HGDIOBJ(pen.0)) };
        unsafe {
            let _ = MoveToEx(target, rect.left, rect.top, None);
            let _ = LineTo(target, rect.right - 1, rect.top);
            let _ = LineTo(target, rect.right - 1, rect.bottom - 1);
            let _ = LineTo(target, rect.left, rect.bottom - 1);
            let _ = LineTo(target, rect.left, rect.top);
            let _ = SelectObject(target, previous);
            let _ = DeleteObject(pen.into());
        }
    }
}

fn draw_ellipse(target: HDC, rect: UiRect, style: VisualStyle) {
    let Some(fill) = style.fill else {
        return;
    };
    let brush = unsafe { CreateSolidBrush(colorref(fill)) };
    let previous = unsafe { SelectObject(target, HGDIOBJ(brush.0)) };
    unsafe {
        let _ = Ellipse(target, rect.left, rect.top, rect.right, rect.bottom);
        let _ = SelectObject(target, previous);
        let _ = DeleteObject(brush.into());
    }
}

fn draw_line(
    target: HDC,
    start: crate::core::Point,
    end: crate::core::Point,
    stroke: crate::core::Stroke,
) {
    let pen = unsafe { CreatePen(PS_SOLID, stroke.width.max(1), colorref(stroke.color)) };
    let previous = unsafe { SelectObject(target, HGDIOBJ(pen.0)) };
    unsafe {
        let _ = MoveToEx(target, start.x, start.y, None);
        let _ = LineTo(target, end.x, end.y);
        let _ = SelectObject(target, previous);
        let _ = DeleteObject(pen.into());
    }
}

fn draw_text(target: HDC, rect: UiRect, text: &str, style: TextStyle) {
    let font = unsafe {
        CreateFontW(
            style.height,
            0,
            0,
            0,
            style.weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_ONLY_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR::null(),
        )
    };
    let previous = unsafe { SelectObject(target, HGDIOBJ(font.0)) };
    unsafe {
        let _ = SetBkMode(target, TRANSPARENT);
        let _ = SetTextColor(target, colorref(style.color));
    }
    let mut native_rect = win_rect(rect);
    let align = match style.align {
        TextAlign::Left => DT_LEFT,
        TextAlign::Center => DT_CENTER,
        TextAlign::Right => DT_RIGHT,
    };
    let mut wide = text.encode_utf16().collect::<Vec<_>>();
    unsafe {
        let _ = DrawTextW(
            target,
            &mut wide,
            &mut native_rect,
            align | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        let _ = SelectObject(target, previous);
        let _ = DeleteObject(font.into());
    }
}

fn fill_rect(target: HDC, rect: UiRect, color: Color) {
    let brush = unsafe { CreateSolidBrush(colorref(color)) };
    unsafe {
        let _ = FillRect(target, &win_rect(rect), brush);
        let _ = DeleteObject(brush.into());
    }
}

fn colorref(color: Color) -> COLORREF {
    let value = color.0;
    COLORREF(((value & 0xFF) << 16) | (value & 0x00FF00) | ((value >> 16) & 0xFF))
}

fn win_rect(rect: UiRect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
