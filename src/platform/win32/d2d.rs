use windows::{
    core::{Result, PCWSTR},
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{
                Common::{D2D1_COLOR_F, D2D_RECT_F, D2D_SIZE_U},
                D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget,
                D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_ROUNDED_RECT,
            },
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
                DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
                DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP,
            },
            Gdi::HDC,
        },
    },
};
use windows_numerics::Vector2;

use crate::core::{
    Color, Scene, ScenePrimitive, Stroke, TextAlign, TextStyle, UiRect, VisualStyle,
};

use super::{Win32Renderer, Win32RendererFactory};

#[derive(Clone, Copy, Debug, Default)]
pub struct D2dRendererFactory;

impl Win32RendererFactory for D2dRendererFactory {
    fn create(&self, hwnd: HWND) -> Result<Box<dyn Win32Renderer>> {
        Ok(Box::new(D2dRenderer::new(hwnd)?))
    }
}

pub struct D2dRenderer {
    target: ID2D1HwndRenderTarget,
    dwrite: IDWriteFactory,
    size: D2D_SIZE_U,
}

impl D2dRenderer {
    pub fn new(hwnd: HWND) -> Result<Self> {
        let factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES::default();
        let hwnd_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: 1,
                height: 1,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let target = unsafe { factory.CreateHwndRenderTarget(&properties, &hwnd_properties) }?;
        let dwrite = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        Ok(Self {
            target,
            dwrite,
            size: hwnd_properties.pixelSize,
        })
    }

    fn resize(&mut self, viewport: UiRect) -> Result<()> {
        let size = D2D_SIZE_U {
            width: viewport.width().max(1) as u32,
            height: viewport.height().max(1) as u32,
        };
        if size != self.size {
            unsafe { self.target.Resize(&size) }?;
            self.size = size;
        }
        Ok(())
    }

    fn draw_commands(&self, commands: &[ScenePrimitive]) -> Result<()> {
        for command in commands {
            match command {
                ScenePrimitive::Rect { rect, style, .. } => self.draw_rect(*rect, *style)?,
                ScenePrimitive::Ellipse { rect, style, .. } => self.draw_ellipse(*rect, *style)?,
                ScenePrimitive::Text {
                    rect, text, style, ..
                } => self.draw_text(*rect, text, *style)?,
                ScenePrimitive::Line {
                    start, end, stroke, ..
                } => self.draw_line(*start, *end, *stroke)?,
                ScenePrimitive::Clip { rect, commands, .. }
                | ScenePrimitive::ClipPath { rect, commands, .. } => unsafe {
                    self.target
                        .PushAxisAlignedClip(&d2d_rect(*rect), D2D1_ANTIALIAS_MODE_ALIASED);
                    self.draw_commands(commands)?;
                    self.target.PopAxisAlignedClip();
                },
                ScenePrimitive::StaticLayer { commands, .. }
                | ScenePrimitive::ScrollRaster { commands, .. } => self.draw_commands(commands)?,
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
        Ok(())
    }

    fn draw_rect(&self, rect: UiRect, style: VisualStyle) -> Result<()> {
        let rect = d2d_rect(rect);
        if let Some(fill) = style.fill {
            let brush = self.brush(fill, style.fill_alpha)?;
            if style.radius > 0 {
                let rounded = D2D1_ROUNDED_RECT {
                    rect,
                    radiusX: style.radius as f32,
                    radiusY: style.radius as f32,
                };
                unsafe { self.target.FillRoundedRectangle(&rounded, &brush) };
            } else {
                unsafe { self.target.FillRectangle(&rect, &brush) };
            }
        }
        if let Some(stroke) = style.stroke {
            let brush = self.brush(stroke.color, stroke.alpha)?;
            unsafe {
                self.target
                    .DrawRectangle(&rect, &brush, stroke.width.max(1) as f32, None)
            };
        }
        Ok(())
    }

    fn draw_ellipse(&self, rect: UiRect, style: VisualStyle) -> Result<()> {
        let ellipse = D2D1_ELLIPSE {
            point: Vector2 {
                X: (rect.left + rect.right) as f32 / 2.0,
                Y: (rect.top + rect.bottom) as f32 / 2.0,
            },
            radiusX: rect.width() as f32 / 2.0,
            radiusY: rect.height() as f32 / 2.0,
        };
        if let Some(fill) = style.fill {
            let brush = self.brush(fill, style.fill_alpha)?;
            unsafe { self.target.FillEllipse(&ellipse, &brush) };
        }
        Ok(())
    }

    fn draw_line(
        &self,
        start: crate::core::Point,
        end: crate::core::Point,
        stroke: Stroke,
    ) -> Result<()> {
        let brush = self.brush(stroke.color, stroke.alpha)?;
        unsafe {
            self.target.DrawLine(
                Vector2 {
                    X: start.x as f32,
                    Y: start.y as f32,
                },
                Vector2 {
                    X: end.x as f32,
                    Y: end.y as f32,
                },
                &brush,
                stroke.width.max(1) as f32,
                None,
            )
        };
        Ok(())
    }

    fn draw_text(&self, rect: UiRect, text: &str, style: TextStyle) -> Result<()> {
        let family = wide("Segoe UI");
        let locale = wide("en-us");
        let format = unsafe {
            self.dwrite.CreateTextFormat(
                PCWSTR(family.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT(style.weight),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.height.unsigned_abs().max(1) as f32,
                PCWSTR(locale.as_ptr()),
            )
        }?;
        unsafe {
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetTextAlignment(match style.align {
                TextAlign::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                TextAlign::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                TextAlign::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            })?;
        }
        let brush = self.brush(style.color, style.alpha)?;
        let text = text.encode_utf16().collect::<Vec<_>>();
        unsafe {
            self.target.DrawText(
                &text,
                &format,
                &d2d_rect(rect),
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        };
        Ok(())
    }

    fn brush(
        &self,
        color: Color,
        alpha: u8,
    ) -> Result<windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush> {
        unsafe {
            self.target
                .CreateSolidColorBrush(&d2d_color(color, alpha), None)
        }
    }
}

impl Win32Renderer for D2dRenderer {
    fn draw(&mut self, _hwnd: HWND, _target: HDC, scene: &Scene, viewport: UiRect) {
        if self.resize(viewport).is_err() {
            return;
        }
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&d2d_color(Color(0x111418), 0xFF)));
        }
        let result = self.draw_commands(scene.commands());
        let presented = unsafe { self.target.EndDraw(None, None) };
        let _ = result.and(presented);
    }
}

fn d2d_rect(rect: UiRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: rect.left as f32,
        top: rect.top as f32,
        right: rect.right as f32,
        bottom: rect.bottom as f32,
    }
}

fn d2d_color(color: Color, alpha: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((color.0 >> 16) & 0xFF) as f32 / 255.0,
        g: ((color.0 >> 8) & 0xFF) as f32 / 255.0,
        b: (color.0 & 0xFF) as f32 / 255.0,
        a: alpha as f32 / 255.0,
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
