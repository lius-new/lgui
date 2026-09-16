use windows::Win32::{
    Foundation::{COLORREF, RECT},
    Graphics::Gdi::{BitBlt, CreateSolidBrush, DeleteObject, FillRect, HDC, SRCCOPY},
};
#[cfg(not(feature = "advanced-rendering"))]
use windows::{
    core::PCWSTR,
    Win32::Graphics::Gdi::{
        CreateFontW, CreatePen, DrawTextW, Ellipse, IntersectClipRect, LineTo, MoveToEx, RestoreDC,
        RoundRect, SaveDC, SelectObject, SetBkMode, SetTextColor, SetViewportOrgEx,
        CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DEFAULT_QUALITY, DT_CENTER, DT_LEFT,
        DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, HGDIOBJ, OUT_TT_ONLY_PRECIS,
        PS_SOLID, TRANSPARENT,
    },
};

#[cfg(feature = "advanced-rendering")]
use std::sync::atomic::{AtomicU64, Ordering};

use super::application::Win32RenderError;
use super::backbuffer::LayeredBackbuffer;

#[cfg(not(feature = "advanced-rendering"))]
use lgui_core::core::{ScenePrimitive, TextAlign, TextStyle, VisualStyle};
use lgui_core::{
    application::RenderErrorStage,
    core::{Color, PhysicalRect, Scene, UiRect},
};

pub struct GdiRenderer {
    background: Color,
    backbuffer: Option<LayeredBackbuffer>,
    #[cfg(not(feature = "advanced-rendering"))]
    shadows: std::cell::RefCell<
        std::collections::HashMap<lgui_core::core::UiId, (u64, LayeredBackbuffer)>,
    >,
    #[cfg(feature = "advanced-rendering")]
    compositing_layer_scope: u64,
}

#[cfg(feature = "advanced-rendering")]
static NEXT_COMPOSITING_LAYER_SCOPE: AtomicU64 = AtomicU64::new(1);

impl GdiRenderer {
    #[cfg(not(feature = "advanced-rendering"))]
    fn draw_shadow(&self, target: HDC, command: &ScenePrimitive) -> bool {
        use std::hash::{Hash, Hasher};
        use windows::Win32::Graphics::Gdi::{
            AlphaBlend, GdiFlush, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION,
        };
        let ScenePrimitive::CompositingLayer {
            id,
            rect,
            spec,
            commands,
            content_signature,
            ..
        } = command
        else {
            return false;
        };
        let Some(shadow) = lgui_core::backend::compositing_shadow(spec) else {
            return false;
        };
        let content_signature =
            lgui_core::backend::resolved_content_signature(commands, *content_signature);
        let width = rect.width().ceil().max(1.0) as i32;
        let height = rect.height().ceil().max(1.0) as i32;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (content_signature, shadow, width, height).hash(&mut hasher);
        let signature = hasher.finish();
        let previous = self.shadows.borrow_mut().remove(id);
        let output = if let Some((_, output)) = previous.filter(|(key, _)| *key == signature) {
            output
        } else {
            let Some(mut black) = LayeredBackbuffer::new(target, width, height) else {
                return false;
            };
            let Some(white) = LayeredBackbuffer::new(target, width, height) else {
                return false;
            };
            let bounds = UiRect::new(0.0, 0.0, width as f32, height as f32);
            fill_rect(black.hdc(), bounds, Color::BLACK);
            fill_rect(white.hdc(), bounds, Color::WHITE);
            self.draw_commands(black.hdc(), commands);
            self.draw_commands(white.hdc(), commands);
            unsafe {
                let _ = GdiFlush();
            }
            let mut pixels = black.pixels().to_vec();
            for (pixel, white) in pixels
                .chunks_exact_mut(4)
                .zip(white.pixels().chunks_exact(4))
            {
                let backdrop = (0..3)
                    .map(|c| white[c].saturating_sub(pixel[c]) as u16)
                    .sum::<u16>();
                let alpha = 255u8.saturating_sub(((backdrop + 1) / 3) as u8);
                for channel in &mut pixel[..3] {
                    *channel = (*channel).min(alpha);
                }
                pixel[3] = alpha;
            }
            lgui_core::backend::composite_shadow(
                &mut pixels,
                width as usize,
                height as usize,
                shadow,
            );
            black.copy_pixels_from(&pixels);
            black
        };
        let result = unsafe {
            AlphaBlend(
                target,
                round_coord(rect.left),
                round_coord(rect.top),
                width,
                height,
                output.hdc(),
                0,
                0,
                width,
                height,
                BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                },
            )
        }
        .as_bool();
        self.shadows
            .borrow_mut()
            .insert(id.clone(), (signature, output));
        result
    }

    pub fn new(background: Color) -> Self {
        Self {
            background,
            backbuffer: None,
            #[cfg(not(feature = "advanced-rendering"))]
            shadows: Default::default(),
            #[cfg(feature = "advanced-rendering")]
            compositing_layer_scope: NEXT_COMPOSITING_LAYER_SCOPE.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn clear(&self, target: HDC, rect: UiRect) {
        fill_rect(target, rect, self.background);
    }

    pub(crate) fn memory_usage(&self) -> lgui_core::memory::CacheUsage {
        let live_bytes = self
            .backbuffer
            .as_ref()
            .map_or(0, LayeredBackbuffer::byte_len);
        let usage = lgui_core::memory::CacheUsage {
            live_bytes,
            cpu_bytes: live_bytes,
            entries: usize::from(self.backbuffer.is_some()),
            largest_entry_bytes: live_bytes,
            ..Default::default()
        };
        #[cfg(not(feature = "advanced-rendering"))]
        let usage = self
            .shadows
            .borrow()
            .values()
            .fold(usage, |mut usage, (_, buffer)| {
                let bytes = buffer.byte_len();
                usage.live_bytes += bytes;
                usage.cpu_bytes += bytes;
                usage.entries += 1;
                usage.largest_entry_bytes = usage.largest_entry_bytes.max(bytes);
                usage
            });
        usage
    }

    #[cfg(not(feature = "advanced-rendering"))]
    fn draw_commands(&self, target: HDC, commands: &[ScenePrimitive]) {
        for command in commands {
            self.draw_command(target, command);
        }
    }

    #[cfg(not(feature = "advanced-rendering"))]
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
                    let rect = win_rect(*rect);
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
            ScenePrimitive::CompositingLayer {
                rect,
                commands,
                spec,
                ..
            } => {
                if lgui_core::backend::compositing_shadow(spec).is_some()
                    && self.draw_shadow(target, command)
                {
                    return;
                }
                let saved = unsafe { SaveDC(target) };
                if saved != 0 {
                    unsafe {
                        let _ = SetViewportOrgEx(
                            target,
                            round_coord(rect.left),
                            round_coord(rect.top),
                            None,
                        );
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

    /// Updates the retained off-screen surface only inside scene damage, then copies the result
    /// to the destination DC. For exposure paints the caller supplies BeginPaint's clipped DC;
    /// changed Host regions use an unclipped client DC so moved nodes can reach their new bounds.
    pub(crate) fn draw_retained(
        &mut self,
        target: HDC,
        scene: &Scene,
        viewport: PhysicalRect,
        damage: &[PhysicalRect],
    ) -> Result<(), Win32RenderError> {
        #[cfg(not(feature = "advanced-rendering"))]
        {
            fn collect(
                commands: &[ScenePrimitive],
                live: &mut std::collections::HashSet<lgui_core::core::UiId>,
            ) {
                for command in commands {
                    match command {
                        ScenePrimitive::CompositingLayer {
                            id, spec, commands, ..
                        } => {
                            if lgui_core::backend::compositing_shadow(spec).is_some() {
                                live.insert(id.clone());
                            }
                            collect(commands, live);
                        }
                        ScenePrimitive::StaticLayer { commands, .. }
                        | ScenePrimitive::ScrollRaster { commands, .. }
                        | ScenePrimitive::Clip { commands, .. }
                        | ScenePrimitive::ClipPath { commands, .. } => collect(commands, live),
                        _ => {}
                    }
                }
            }
            let mut live = std::collections::HashSet::new();
            collect(scene.commands(), &mut live);
            self.shadows.borrow_mut().retain(|id, _| live.contains(id));
        }
        let replace = self.backbuffer.as_ref().is_none_or(|buffer| {
            buffer.width() != viewport.width() || buffer.height() != viewport.height()
        });
        if replace {
            self.backbuffer = LayeredBackbuffer::new(target, viewport.width(), viewport.height());
        }

        let Some(mut backbuffer) = self.backbuffer.take() else {
            self.draw_direct(target, scene, viewport, None);
            return Ok(());
        };

        let mut repaint = if backbuffer.is_valid() {
            damage
                .iter()
                .filter_map(|rect| rect.intersect(viewport))
                .collect::<Vec<_>>()
        } else {
            vec![viewport]
        };

        for rect in &repaint {
            self.draw_direct(backbuffer.hdc(), scene, viewport, Some(*rect));
        }
        if !backbuffer.is_valid() || !repaint.is_empty() {
            backbuffer.mark_valid();
        }

        // A clean Host commit may still be a Win32 exposure paint. In that case the retained
        // surface is copied without rebuilding any scene pixels; the paint DC clips the blit to
        // the actual update region.
        if repaint.is_empty() {
            repaint.push(viewport);
        }
        let presented = repaint.iter().try_for_each(|rect| unsafe {
            BitBlt(
                target,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                Some(backbuffer.hdc()),
                rect.left,
                rect.top,
                SRCCOPY,
            )
            .map_err(|source| {
                Win32RenderError::new(RenderErrorStage::Present, "bitblt_backbuffer", source)
            })
        });
        self.backbuffer = Some(backbuffer);
        presented
    }

    fn draw_direct(
        &mut self,
        target: HDC,
        scene: &Scene,
        viewport: PhysicalRect,
        clip: Option<PhysicalRect>,
    ) {
        let clear = clip.unwrap_or(viewport).as_ui_rect();
        self.clear(target, clear);
        let clip = clip.map(PhysicalRect::as_ui_rect);
        #[cfg(feature = "advanced-rendering")]
        super::enhanced::GdiRenderer::draw_scene_clipped_scoped(
            target,
            scene,
            clip,
            self.compositing_layer_scope,
        );
        #[cfg(not(feature = "advanced-rendering"))]
        self.draw_scene_clipped(target, scene, clip);
    }

    #[cfg(not(feature = "advanced-rendering"))]
    fn draw_scene_clipped(&self, target: HDC, scene: &Scene, clip: Option<UiRect>) {
        let saved = clip.map(|clip| unsafe {
            let saved = SaveDC(target);
            if saved != 0 {
                let clip = win_rect(clip);
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

impl Default for GdiRenderer {
    fn default() -> Self {
        Self::new(Color(0x111418))
    }
}

impl Drop for GdiRenderer {
    fn drop(&mut self) {
        #[cfg(feature = "advanced-rendering")]
        super::enhanced::release_gdi_compositing_layer_scope(self.compositing_layer_scope);
    }
}

#[cfg(not(feature = "advanced-rendering"))]
fn draw_rect(target: HDC, rect: UiRect, style: VisualStyle) {
    let native_rect = win_rect(rect);
    if let Some(fill) = style.fill {
        if style.radius > 0.0 {
            let brush = unsafe { CreateSolidBrush(colorref(fill)) };
            let previous = unsafe { SelectObject(target, HGDIOBJ(brush.0)) };
            let diameter = (style.radius * 2.0).round().max(1.0) as i32;
            unsafe {
                let _ = RoundRect(
                    target,
                    native_rect.left,
                    native_rect.top,
                    native_rect.right,
                    native_rect.bottom,
                    diameter,
                    diameter,
                );
                let _ = SelectObject(target, previous);
                let _ = DeleteObject(brush.into());
            }
        } else {
            fill_rect(target, rect, fill);
        }
    }
    if let Some(stroke) = style.stroke {
        let pen_width = stroke.width.round().max(1.0) as i32;
        let pen = unsafe { CreatePen(PS_SOLID, pen_width, colorref(stroke.color)) };
        let previous = unsafe { SelectObject(target, HGDIOBJ(pen.0)) };
        unsafe {
            let _ = MoveToEx(target, native_rect.left, native_rect.top, None);
            let _ = LineTo(target, native_rect.right - 1, native_rect.top);
            let _ = LineTo(target, native_rect.right - 1, native_rect.bottom - 1);
            let _ = LineTo(target, native_rect.left, native_rect.bottom - 1);
            let _ = LineTo(target, native_rect.left, native_rect.top);
            let _ = SelectObject(target, previous);
            let _ = DeleteObject(pen.into());
        }
    }
}

#[cfg(not(feature = "advanced-rendering"))]
fn draw_ellipse(target: HDC, rect: UiRect, style: VisualStyle) {
    let Some(fill) = style.fill else {
        return;
    };
    let brush = unsafe { CreateSolidBrush(colorref(fill)) };
    let previous = unsafe { SelectObject(target, HGDIOBJ(brush.0)) };
    let rect = win_rect(rect);
    unsafe {
        let _ = Ellipse(target, rect.left, rect.top, rect.right, rect.bottom);
        let _ = SelectObject(target, previous);
        let _ = DeleteObject(brush.into());
    }
}

#[cfg(not(feature = "advanced-rendering"))]
fn draw_line(
    target: HDC,
    start: lgui_core::core::Point,
    end: lgui_core::core::Point,
    stroke: lgui_core::core::Stroke,
) {
    let pen_width = stroke.width.round().max(1.0) as i32;
    let pen = unsafe { CreatePen(PS_SOLID, pen_width, colorref(stroke.color)) };
    let previous = unsafe { SelectObject(target, HGDIOBJ(pen.0)) };
    unsafe {
        let _ = MoveToEx(target, round_coord(start.x), round_coord(start.y), None);
        let _ = LineTo(target, round_coord(end.x), round_coord(end.y));
        let _ = SelectObject(target, previous);
        let _ = DeleteObject(pen.into());
    }
}

#[cfg(not(feature = "advanced-rendering"))]
fn draw_text(target: HDC, rect: UiRect, text: &str, style: TextStyle) {
    let font = unsafe {
        CreateFontW(
            style.height.round() as i32,
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

#[cfg(not(feature = "advanced-rendering"))]
fn round_coord(value: f32) -> i32 {
    value.round() as i32
}

fn win_rect(rect: UiRect) -> RECT {
    let rect = PhysicalRect::new(
        rect.left.floor() as i32,
        rect.top.floor() as i32,
        rect.right.ceil() as i32,
        rect.bottom.ceil() as i32,
    );
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[cfg(all(test, not(feature = "advanced-rendering")))]
#[path = "gdi_shadow_tests_test.rs"]
mod shadow_tests;
