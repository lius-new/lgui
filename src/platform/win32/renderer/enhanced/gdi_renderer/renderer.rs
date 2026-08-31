use super::*;

pub struct GdiRenderer;

impl GdiRenderer {
    pub fn draw_scene(hdc: HDC, list: &Scene) {
        Self::draw_scene_clipped(hdc, list, None);
    }

    pub fn draw_scene_clipped(hdc: HDC, list: &Scene, clip: Option<UiRect>) {
        Self::draw_scene_clipped_scoped(hdc, list, clip, 0);
    }

    pub fn draw_scene_clipped_scoped(hdc: HDC, list: &Scene, clip: Option<UiRect>, scope: u64) {
        GDI_COMPOSITING_LAYER_SCOPE.with(|current| current.set(scope));
        let mut layer_ids = std::collections::HashSet::new();
        collect_compositing_layer_ids(list.commands(), &mut layer_ids);
        GDI_COMPOSITING_LAYERS.with(|layers| {
            layers
                .borrow_mut()
                .retain(|key, _| key.scope != scope || layer_ids.contains(&key.id));
        });
        let _clip_guard = ClipGuard::new(hdc, clip);
        let clip_region = clip.map(ClipRegion::new);
        for command in list.commands() {
            if clip_region.is_some_and(|clip| !clip.intersects(command)) {
                continue;
            }
            Self::draw_command_clipped(hdc, command, clip);
        }
        publish_gdi_compositing_usage();
    }

    pub fn draw_command(hdc: HDC, command: &ScenePrimitive) {
        Self::draw_command_clipped(hdc, command, None);
    }

    pub(super) fn draw_command_clipped(hdc: HDC, command: &ScenePrimitive, clip: Option<UiRect>) {
        match command {
            ScenePrimitive::Rect { rect, style, .. } => draw_rect(hdc, *rect, *style),
            ScenePrimitive::Ellipse { rect, style, .. } => draw_ellipse(hdc, *rect, *style),
            ScenePrimitive::Text {
                rect, text, style, ..
            } => draw_text(hdc, *rect, text, *style),
            ScenePrimitive::Line {
                start, end, stroke, ..
            } => draw_line(hdc, *start, *end, *stroke),
            ScenePrimitive::Path { path, style, .. } => draw_path(hdc, path, *style),
            ScenePrimitive::Icon {
                rect, key, style, ..
            } => draw_svg_icon(hdc, key, *rect, *style),
            ScenePrimitive::Image {
                rect,
                source,
                request,
                fit,
                ..
            } => {
                let _ = crate::assets::request_image(request);
                image::draw_ui_image(hdc, *rect, source, *fit);
            }
            ScenePrimitive::Overlay { rect, style, .. } => draw_overlay(hdc, *rect, style),
            ScenePrimitive::CompositingLayer {
                id,
                rect,
                spec,
                commands,
                content_signature,
                ..
            } => draw_gdi_compositing_layer(
                hdc,
                id,
                *rect,
                clip,
                *spec,
                commands,
                *content_signature,
            ),
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                draw_backdrop_blur(hdc, *rect, *style, clip)
            }
            ScenePrimitive::BackdropBlurPath {
                rect, path, style, ..
            } => draw_backdrop_blur_path(hdc, *rect, path, *style, clip),
            ScenePrimitive::Custom {
                rect, key, style, ..
            } => {
                if let (Some(style), Some(provider)) = (
                    style,
                    crate::assets::render_resources().custom_paint().cloned(),
                ) {
                    if let Ok(Some(fragment)) = provider.record(key, *rect, *style) {
                        for command in fragment.commands() {
                            Self::draw_command_clipped(hdc, command, clip);
                        }
                    }
                }
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                if let Some(clip) = clip {
                    if static_layer::draw_static_layer_region::<GdiStaticLayerBackend>(
                        hdc,
                        id,
                        *rect,
                        clip,
                        spec,
                        commands,
                        *child_signature,
                    ) {
                        return;
                    }
                }
                static_layer::draw_static_layer::<GdiStaticLayerBackend>(
                    hdc,
                    id,
                    *rect,
                    spec,
                    commands,
                    *child_signature,
                )
            }
            ScenePrimitive::ScrollRaster {
                id,
                viewport,
                spec,
                commands,
                child_signature,
                ..
            } => static_layer::draw_scroll_raster::<GdiStaticLayerBackend>(
                hdc,
                id,
                *viewport,
                spec,
                commands,
                *child_signature,
            ),
            ScenePrimitive::Clip { rect, commands, .. } => {
                let nested_clip = match clip {
                    Some(clip) => {
                        let Some(intersection) = clip.intersect(*rect) else {
                            return;
                        };
                        Some(intersection)
                    }
                    None => Some(*rect),
                };
                let _clip_guard = ClipGuard::new(hdc, nested_clip);
                let clip_region = nested_clip.map(ClipRegion::new);
                for command in commands {
                    if clip_region.is_some_and(|clip| !clip.intersects(command)) {
                        continue;
                    }
                    Self::draw_command_clipped(hdc, command, nested_clip);
                }
            }
            ScenePrimitive::ClipPath {
                rect,
                path,
                commands,
                ..
            } => {
                let nested_clip = match clip {
                    Some(clip) => {
                        let Some(intersection) = clip.intersect(*rect) else {
                            return;
                        };
                        Some(intersection)
                    }
                    None => Some(*rect),
                };
                let Some(_clip_guard) = PolygonClipGuard::new(hdc, path, nested_clip) else {
                    return;
                };
                let clip_region = nested_clip.map(ClipRegion::new);
                for command in commands {
                    if clip_region.is_some_and(|clip| !clip.intersects(command)) {
                        continue;
                    }
                    Self::draw_command_clipped(hdc, command, nested_clip);
                }
            }
            ScenePrimitive::Glow { .. } => {}
        }
    }
}

pub(super) fn collect_compositing_layer_ids(
    commands: &[ScenePrimitive],
    ids: &mut std::collections::HashSet<UiId>,
) {
    for command in commands {
        match command {
            ScenePrimitive::CompositingLayer { id, commands, .. } => {
                ids.insert(id.clone());
                collect_compositing_layer_ids(commands, ids);
            }
            ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_compositing_layer_ids(commands, ids);
            }
            _ => {}
        }
    }
}

pub(super) fn draw_gdi_compositing_layer(
    hdc: HDC,
    id: &UiId,
    rect: UiRect,
    clip: Option<UiRect>,
    spec: CompositingLayerSpec,
    commands: &[ScenePrimitive],
    content_signature: u64,
) {
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    let key = GdiCompositingLayerKey {
        scope: GDI_COMPOSITING_LAYER_SCOPE.with(Cell::get),
        id: id.clone(),
    };
    let previous = GDI_COMPOSITING_LAYERS.with(|layers| layers.borrow_mut().remove(&key));
    let mut layer = match previous {
        Some(layer)
            if layer.width == width
                && layer.height == height
                && layer.background == spec.background =>
        {
            layer
        }
        _ => {
            let Some(layer) = GdiCompositingLayer::new(hdc, width, height, spec.background) else {
                draw_gdi_compositing_layer_fallback(hdc, rect, clip, commands);
                return;
            };
            layer
        }
    };

    if layer.content_signature != Some(content_signature) {
        let bounds = UiRect::new(0.0, 0.0, width as f32, height as f32);
        let damage = if layer.commands.is_empty() {
            vec![bounds]
        } else {
            compositing_layer_damage(&layer.commands, commands, bounds)
        };
        layer.redraw(commands, &damage);
        layer.content_signature = Some(content_signature);
        layer.commands = commands.to_vec();
    }

    if spec.opacity == 0 {
        GDI_COMPOSITING_LAYERS.with(|layers| {
            layers.borrow_mut().insert(key, layer);
        });
        return;
    }

    if !spec.transform.is_identity() {
        let bounds = spec.transform.transformed_bounds(rect);
        if clip.and_then(|clip| clip.intersect(bounds)).is_none() && clip.is_some() {
            GDI_COMPOSITING_LAYERS.with(|layers| {
                layers.borrow_mut().insert(key, layer);
            });
            return;
        }
        record_gdi_frame_blit(
            GdiFrameBlitSource::StaticLayer,
            GdiFrameBlitKind::AlphaBlend,
            bounds,
        );
        draw_gdi_transformed_bitmap(hdc, rect, clip, &layer.output, spec.opacity, spec.transform);
        GDI_COMPOSITING_LAYERS.with(|layers| {
            layers.borrow_mut().insert(key, layer);
        });
        return;
    }

    let dest = match clip {
        Some(clip) => {
            let Some(dest) = rect.intersect(clip) else {
                GDI_COMPOSITING_LAYERS.with(|layers| {
                    layers.borrow_mut().insert(key, layer);
                });
                return;
            };
            dest
        }
        None => rect,
    };
    let source = UiRect::new(
        dest.left - rect.left,
        dest.top - rect.top,
        dest.right - rect.left,
        dest.bottom - rect.top,
    );
    let dest_px = pixel_rect_outward(dest);
    let source_px = pixel_rect_outward(source);
    unsafe {
        if spec.opacity == 255 && layer.output.opaque {
            record_gdi_frame_blit(
                GdiFrameBlitSource::StaticLayer,
                GdiFrameBlitKind::BitBlt,
                dest,
            );
            let _ = BitBlt(
                hdc,
                dest_px.left,
                dest_px.top,
                dest_px.width(),
                dest_px.height(),
                Some(layer.output.memory_dc),
                source_px.left,
                source_px.top,
                SRCCOPY,
            );
        } else {
            record_gdi_frame_blit(
                GdiFrameBlitSource::StaticLayer,
                GdiFrameBlitKind::AlphaBlend,
                dest,
            );
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: spec.opacity,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = AlphaBlend(
                hdc,
                dest_px.left,
                dest_px.top,
                dest_px.width(),
                dest_px.height(),
                layer.output.memory_dc,
                source_px.left,
                source_px.top,
                source_px.width(),
                source_px.height(),
                blend,
            );
        }
    }
    GDI_COMPOSITING_LAYERS.with(|layers| {
        layers.borrow_mut().insert(key, layer);
    });
}
