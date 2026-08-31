impl D2dRenderer {
    pub fn new(
        context: ID2D1DeviceContext,
        dwrite_factory: IDWriteFactory,
        width: i32,
        height: i32,
    ) -> Result<Self> {
        let scene_bitmap = create_scene_bitmap(&context, width, height)?;
        unsafe {
            context.SetTarget(&scene_bitmap);
        }
        Ok(Self {
            context,
            dwrite_factory,
            scene_bitmap,
            bitmap_cache: D2dBitmapCache::new(d2d_bitmap_cache_budget(width, height)),
            overlay_brush_cache: HashMap::new(),
            frame_bitmap_cache: HashMap::new(),
            compositing_layers: HashMap::new(),
        })
    }

    pub fn draw_scene_full(&mut self, list: &Scene) -> Result<()> {
        self.begin_frame(list);
        self.retain_compositing_layers(list);
        self.ensure_static_layer_cache(list, None)?;
        unsafe {
            self.context.SetTarget(&self.scene_bitmap);
            self.context.BeginDraw();
            self.context.Clear(Some(&transparent()));
            draw_scene_d2d(self, list, None)?;
            self.context.EndDraw(None, None)?;
        }
        self.bitmap_cache.evict_to_budget();
        Ok(())
    }

    pub fn draw_scene_dirty(&mut self, list: &Scene, rects: &[UiRect]) -> Result<()> {
        self.begin_frame(list);
        self.retain_compositing_layers(list);
        for rect in rects {
            self.ensure_static_layer_cache(list, Some(*rect))?;
        }
        unsafe {
            self.context.SetTarget(&self.scene_bitmap);
            self.context.BeginDraw();
            for rect in rects {
                let clip = d2d_rect(*rect);
                self.context
                    .PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
                self.context.Clear(Some(&transparent()));
                draw_scene_d2d(self, list, Some(*rect))?;
                self.context.PopAxisAlignedClip();
            }
            self.context.EndDraw(None, None)?;
        }
        self.bitmap_cache.evict_to_budget();
        Ok(())
    }

    fn begin_frame(&mut self, list: &Scene) {
        self.frame_bitmap_cache.clear();
        let live = bitmap_cache_keys(list.commands());
        self.bitmap_cache.retain_live(&live);
        let live_overlays = overlay_brush_cache_keys(list.commands());
        self.overlay_brush_cache
            .retain(|key, _| live_overlays.contains(key));
    }

    pub fn copy_scene_to_target(
        &mut self,
        target_bitmap: &ID2D1Bitmap1,
        rects: Option<&[UiRect]>,
    ) -> Result<()> {
        let copy_start = Instant::now();
        unsafe {
            self.context.SetTarget(target_bitmap);
            self.context.BeginDraw();
            match rects {
                Some(rects) => {
                    for rect in rects {
                        let area = d2d_rect(*rect);
                        self.context
                            .PushAxisAlignedClip(&area, D2D1_ANTIALIAS_MODE_ALIASED);
                        self.context.DrawBitmap(
                            &self.scene_bitmap,
                            Some(&area),
                            1.0,
                            D2D1_INTERPOLATION_MODE_LINEAR,
                            Some(&area),
                            None,
                        );
                        self.context.PopAxisAlignedClip();
                    }
                }
                None => {
                    self.context.DrawBitmap(
                        &self.scene_bitmap,
                        None,
                        1.0,
                        D2D1_INTERPOLATION_MODE_LINEAR,
                        None,
                        None,
                    );
                }
            }
            self.context.EndDraw(None, None)?;
        }
        match rects {
            Some(rects) => {
                trace_d2d_regions("presenter.d2d.copy.dirty", Some(rects));
                trace_duration("presenter.d2d.copy.dirty", copy_start.elapsed());
            }
            None => {
                trace_d2d_regions("presenter.d2d.copy.full", None);
                trace_duration("presenter.d2d.copy.full", copy_start.elapsed());
            }
        }
        Ok(())
    }

    fn ensure_static_layer_cache(&mut self, list: &Scene, clip: Option<UiRect>) -> Result<()> {
        for command in list.commands() {
            self.ensure_static_layer_command(command, clip)?;
        }
        Ok(())
    }

    fn retain_compositing_layers(&mut self, list: &Scene) {
        let mut live = std::collections::HashSet::new();
        collect_compositing_layer_ids(list.commands(), &mut live);
        self.compositing_layers.retain(|id, _| live.contains(id));
    }

    fn ensure_static_layer_command(
        &mut self,
        command: &ScenePrimitive,
        clip: Option<UiRect>,
    ) -> Result<()> {
        if clip
            .and_then(|clip| clip.intersect(command.paint_bounds()))
            .is_none()
            && clip.is_some()
        {
            return Ok(());
        }
        match command {
            ScenePrimitive::CompositingLayer {
                id,
                rect,
                spec,
                commands,
                content_signature,
                ..
            } => {
                let (width, height) = raster_size(*rect);
                let previous = self.compositing_layers.remove(id);
                let mut layer = match previous {
                    Some(layer)
                        if layer.width == width
                            && layer.height == height
                            && layer.background == spec.background =>
                    {
                        layer
                    }
                    _ => create_compositing_layer(self, width, height, spec.background)?,
                };
                if layer.content_signature != Some(*content_signature) {
                    for command in commands {
                        self.ensure_static_layer_command(command, None)?;
                    }
                    let bounds = UiRect::new(0.0, 0.0, width as f32, height as f32);
                    let damage = if layer.commands.is_empty() {
                        vec![bounds]
                    } else {
                        compositing_layer_damage(&layer.commands, commands, bounds)
                    };
                    redraw_compositing_layer(
                        self,
                        &layer.bitmap,
                        spec.background,
                        commands,
                        &damage,
                    )?;
                    layer.content_signature = Some(*content_signature);
                    layer.commands = commands.clone();
                }
                self.compositing_layers.insert(id.clone(), layer);
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                if pure_static_layer_image(spec, commands).is_some() {
                    return Ok(());
                }
                let cache_key = static_layer_cache_key(
                    id,
                    spec,
                    raster_length(rect.width()),
                    raster_length(rect.height()),
                    *child_signature,
                );
                if spec.cache_policy == StaticLayerCachePolicy::Disabled {
                    if !self.frame_bitmap_cache.contains_key(&cache_key) {
                        for command in commands {
                            self.ensure_static_layer_command(command, clip)?;
                        }
                        let bitmap = render_static_layer_bitmap(
                            self,
                            *rect,
                            spec,
                            commands,
                            Some(&cache_key),
                        )?;
                        self.frame_bitmap_cache.insert(cache_key, bitmap);
                    }
                } else if self.bitmap_cache.get(&cache_key).is_none() {
                    for command in commands {
                        self.ensure_static_layer_command(command, clip)?;
                    }
                    let bitmap =
                        render_static_layer_bitmap(self, *rect, spec, commands, Some(&cache_key))?;
                    self.bitmap_cache.insert(cache_key, bitmap);
                }
            }
            ScenePrimitive::Clip { rect, commands, .. } => {
                let nested_clip = clip.and_then(|clip| clip.intersect(*rect)).or(Some(*rect));
                for command in commands {
                    self.ensure_static_layer_command(command, nested_clip)?;
                }
            }
            ScenePrimitive::ScrollRaster {
                viewport,
                spec,
                commands,
                ..
            } => {
                let nested_clip = clip
                    .and_then(|clip| clip.intersect(*viewport))
                    .or(Some(*viewport));
                let translated = commands
                    .iter()
                    .map(|command| translate_command(command, 0.0, -spec.scroll_y))
                    .collect::<Vec<_>>();
                for command in &translated {
                    self.ensure_static_layer_command(command, nested_clip)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
