pub(crate) struct SkiaSoftwareSurface {
    pixels: Vec<u8>,
    size: (i32, i32),
    cache: SkiaCache,
}

impl SkiaSoftwareSurface {
    pub(crate) fn new(cache_budget: usize) -> Self {
        Self {
            pixels: Vec::new(),
            size: (0, 0),
            cache: SkiaCache::new(cache_budget),
        }
    }

    fn ensure_surface(&mut self, width: i32, height: i32) {
        let size = (width.max(1), height.max(1));
        if self.size != size {
            self.size = size;
            self.pixels.resize(size.0 as usize * size.1 as usize * 4, 0);
            self.cache.trim(MemoryPressure::Critical);
        }
    }

    pub(crate) fn draw(&mut self, scene: &Scene, frame: &FrameInfo<'_>) -> Result<(), String> {
        let viewport = frame.viewport();
        self.ensure_surface(viewport.width(), viewport.height());
        let info = ImageInfo::new(self.size, ColorType::BGRA8888, AlphaType::Premul, None);
        let mut surface = surfaces::wrap_pixels(
            &info,
            self.pixels.as_mut_slice(),
            self.size.0 as usize * 4,
            None,
        )
        .ok_or_else(|| "Skia could not wrap the retained software surface".to_owned())?;
        paint_scene_damage(surface.canvas(), &mut self.cache, scene, frame)
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn size(&self) -> (i32, i32) {
        self.size
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        self.cache.trim(pressure);
        if pressure == MemoryPressure::Critical {
            self.pixels.clear();
            self.size = (0, 0);
        }
    }

    pub(crate) fn cache_stats(&self) -> SkiaCacheStats {
        self.cache.stats()
    }
}

pub(crate) fn paint_scene_damage(
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
        canvas.clear(SkColor::TRANSPARENT);
        painter.draw_commands(canvas, scene.commands(), Some(ui_rect_from_physical(clip)))?;
        canvas.restore();
    }
    Ok(())
}
