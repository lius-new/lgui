use super::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static OPAQUE_WINDOWS: AtomicBool = AtomicBool::new(false);
static TEXT_BACKGROUND: AtomicU32 = AtomicU32::new(0xffffff);

/// Opt in before creating native windows whose presenter discards alpha.
/// Standalone/offscreen renderer consumers retain their transparent semantics.
pub fn set_opaque_software_windows(opaque: bool) {
    OPAQUE_WINDOWS.store(opaque, Ordering::Relaxed);
}

/// Supply the active UI background before drawing native software windows.
/// Transparent/offscreen consumers retain Skia's original coverage curve.
pub fn set_software_text_background(background: Color) {
    TEXT_BACKGROUND.store(background.0, Ordering::Relaxed);
}

pub struct SkiaSoftwareSurface {
    pixels: Vec<u8>,
    size: (i32, i32),
    cache: SkiaCache,
    opaque: bool,
    last_text_props: Option<(skia_safe::PixelGeometry, u32)>,
}

impl SkiaSoftwareSurface {
    pub fn new(cache_budget: usize) -> Self {
        Self {
            pixels: Vec::new(),
            size: (0, 0),
            cache: SkiaCache::new(cache_budget),
            opaque: OPAQUE_WINDOWS.load(Ordering::Relaxed),
            last_text_props: None,
        }
    }

    fn ensure_surface(&mut self, width: i32, height: i32) {
        let size = (width.max(1), height.max(1));
        if self.size != size {
            self.size = size;
            self.last_text_props = None;
            self.pixels.resize(size.0 as usize * size.1 as usize * 4, 0);
            self.cache.trim(MemoryPressure::Critical);
        }
    }

    pub fn draw(&mut self, scene: &Scene, frame: &FrameInfo<'_>) -> Result<(), String> {
        self.draw_on_background(scene, frame, Color(TEXT_BACKGROUND.load(Ordering::Relaxed)))
    }

    fn draw_on_background(
        &mut self,
        scene: &Scene,
        frame: &FrameInfo<'_>,
        background: Color,
    ) -> Result<(), String> {
        let viewport = frame.viewport();
        self.ensure_surface(viewport.width(), viewport.height());
        // softbuffer presents RGB and discards alpha. Describe that actual opaque
        // destination to Skia; transparent offscreen layers remain premultiplied.
        let alpha = if self.opaque {
            AlphaType::Opaque
        } else {
            AlphaType::Premul
        };
        let info = ImageInfo::new(self.size, ColorType::BGRA8888, alpha, None);
        let props = if self.opaque {
            TextRasterization::system().surface_props_for_background(background)
        } else {
            TextRasterization::Grayscale.surface_props()
        };
        let text_props = (props.pixel_geometry(), props.text_gamma().to_bits());
        // A gamma/pixel-order change affects every retained glyph, even when
        // the caller only invalidated a small part of the window.
        let refresh = FrameInfo::new(
            viewport,
            frame.damage(),
            frame.scale(),
            frame.reason(),
            frame.is_full_redraw() || self.last_text_props != Some(text_props),
        );
        let mut surface = surfaces::wrap_pixels(
            &info,
            self.pixels.as_mut_slice(),
            self.size.0 as usize * 4,
            Some(&props),
        )
        .ok_or_else(|| "Skia could not wrap the retained software surface".to_owned())?;
        paint_scene_damage(surface.canvas(), &mut self.cache, scene, &refresh)?;
        self.last_text_props = Some(text_props);
        Ok(())
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    pub fn trim(&mut self, pressure: MemoryPressure) {
        self.cache.trim(pressure);
        if pressure == MemoryPressure::Critical {
            self.pixels.clear();
            self.size = (0, 0);
            self.last_text_props = None;
        }
    }

    pub fn set_cache_budget(&mut self, budget_bytes: usize) {
        self.cache.set_budget(budget_bytes);
    }

    /// Connects the render cache to the application's observer.
    pub fn set_cache_hooks(&mut self, hooks: Option<lgui_core::memory::CacheHooks>) {
        self.cache.set_hooks(hooks);
    }

    pub fn cache_stats(&self) -> SkiaCacheStats {
        self.cache.stats()
    }
}

pub fn paint_scene_damage(
    canvas: &Canvas,
    cache: &mut SkiaCache,
    scene: &Scene,
    frame: &FrameInfo<'_>,
) -> Result<(), String> {
    cache.begin_frame();
    let clips: Vec<PhysicalRect> = if frame.is_full_redraw() {
        vec![frame.viewport()]
    } else {
        frame.damage().to_vec()
    };
    let mut painter = SkiaPainter { cache };
    for clip in clips {
        canvas.save();
        canvas.clip_rect(physical_rect(clip), None, false);
        // Preserve the target's alpha contract. The native window is opaque;
        // transparent offscreen/test canvases still clear to transparent.
        canvas.clear(if canvas.image_info().alpha_type() == AlphaType::Opaque {
            SkColor::BLACK
        } else {
            SkColor::TRANSPARENT
        });
        painter.draw_commands(canvas, scene.commands(), Some(ui_rect_from_physical(clip)))?;
        canvas.restore();
    }
    painter.cache.end_frame();
    Ok(())
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use lgui_core::core::{RenderPhase, UiId, UiScale};
    use lgui_render_api::FrameReason;

    #[test]
    fn coverage_changes_refresh_cached_text_outside_partial_damage() {
        let scale = UiScale::new(1.0);
        let bounds = UiRect::new(0.0, 0.0, 420.0, 48.0);
        let viewport = scale.physical_rect(bounds);
        let mut scene = Scene::new();
        scene.push(ScenePrimitive::Rect {
            id: UiId::owned("coverage-background"),
            rect: bounds,
            style: VisualStyle::filled(Color::BLACK),
            phase: RenderPhase::Content,
        });
        scene.push(ScenePrimitive::Text {
            id: UiId::owned("coverage-cached-text"),
            rect: UiRect::new(100.0, 0.0, 420.0, 48.0),
            text: "English 中文 00 0689".into(),
            style: TextStyle::new(Color::WHITE, 13.0, 400),
            phase: RenderPhase::Content,
        });
        let damage = [scale.physical_rect(UiRect::new(0.0, 0.0, 8.0, 48.0))];
        let partial = FrameInfo::new(viewport, &damage, scale, FrameReason::SceneChange, false);
        let full = FrameInfo::new(viewport, &[], scale, FrameReason::SceneChange, true);
        let mut retained = SkiaSoftwareSurface::new(1024 * 1024);
        retained.opaque = true;
        for background in [Color::WHITE, Color::BLACK, Color::WHITE] {
            // Keep the scene fixed to isolate a coverage-profile change. All
            // text is outside the supplied damage; compare to a fresh full draw.
            retained
                .draw_on_background(&scene, &partial, background)
                .unwrap();
            let mut fresh = SkiaSoftwareSurface::new(1024 * 1024);
            fresh.opaque = true;
            fresh.draw_on_background(&scene, &full, background).unwrap();
            assert_eq!(retained.pixels(), fresh.pixels());
        }
        assert_eq!(retained.cache_stats().text_entries, 1);
        assert_eq!(retained.cache_stats().text_hits, 2);
    }

    #[test]
    fn opaque_window_first_frame_and_cached_repaint_survive_at_fractional_dpi() {
        let mut renderer = SkiaSoftwareSurface::new(8 * 1024 * 1024);
        // Exercise the startup target without changing the process-global option
        // used by unrelated transparent renderer tests running in parallel.
        renderer.opaque = true;
        let mut logical = Scene::new();
        for (row, sample) in ["Native API started", "代码编辑器", "00 0689"]
            .iter()
            .enumerate()
        {
            logical.push(ScenePrimitive::Text {
                id: UiId::owned(format!("startup-text-{row}")),
                rect: UiRect::new(4.0, row as f32 * 28.0, 400.0, (row + 1) as f32 * 28.0),
                text: (*sample).to_owned().into(),
                style: TextStyle::new(Color::WHITE, 14.0, 400),
                phase: RenderPhase::Content,
            });
        }
        for factor in [0.8, 1.0, 1.25, 1.5] {
            let scale = UiScale::new(factor);
            let scene = logical.project_to_physical(scale);
            let viewport = scale.physical_rect(UiRect::new(0.0, 0.0, 420.0, 96.0));
            for _ in 0..2 {
                let frame = FrameInfo::new(viewport, &[], scale, FrameReason::SceneChange, true);
                renderer.draw(&scene, &frame).unwrap();
                assert!(renderer.pixels().chunks_exact(4).all(|p| p[3] == 255));
                assert!(
                    renderer
                        .pixels()
                        .chunks_exact(4)
                        .any(|p| p[..3].iter().any(|v| *v > 32))
                );
            }
            assert!(renderer.cache_stats().text_hits >= 3);
        }
    }
}
