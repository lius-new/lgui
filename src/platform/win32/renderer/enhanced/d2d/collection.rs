use super::*;

pub(super) fn bitmap_cache_keys(commands: &[ScenePrimitive]) -> HashSet<D2dBitmapCacheKey> {
    let mut keys = HashSet::new();
    collect_bitmap_cache_keys(commands, &mut keys);
    keys
}

pub(super) fn overlay_brush_cache_keys(
    commands: &[ScenePrimitive],
) -> HashSet<D2dOverlayBrushCacheKey> {
    let mut keys = HashSet::new();
    collect_overlay_brush_cache_keys(commands, &mut keys);
    keys
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

pub(super) fn collect_bitmap_cache_keys(
    commands: &[ScenePrimitive],
    keys: &mut HashSet<D2dBitmapCacheKey>,
) {
    for command in commands {
        match command {
            ScenePrimitive::Image {
                rect, source, fit, ..
            } => {
                keys.insert(image_cache_key(*rect, source, *fit));
            }
            ScenePrimitive::Icon {
                rect, key, style, ..
            } => {
                keys.insert(icon_cache_key(*rect, key, *style));
            }
            ScenePrimitive::BackdropBlur { rect, style, .. } => {
                keys.insert(backdrop_blur_cache_key(*rect, *style));
            }
            ScenePrimitive::BackdropBlurPath {
                rect, path, style, ..
            } => {
                keys.insert(backdrop_blur_path_cache_key(*rect, path, *style));
            }
            ScenePrimitive::StaticLayer {
                id,
                rect,
                spec,
                commands,
                child_signature,
                ..
            } => {
                if let Some((source, fit)) = pure_static_layer_image(spec, commands) {
                    keys.insert(image_cache_key(
                        UiRect::new(0.0, 0.0, rect.width(), rect.height()),
                        &UiImageSource::Static(source),
                        fit,
                    ));
                } else if spec.cache_policy == RasterCachePolicy::Disabled {
                    collect_bitmap_cache_keys(commands, keys);
                } else {
                    keys.insert(static_layer_cache_key(
                        id,
                        spec,
                        raster_length(rect.width()),
                        raster_length(rect.height()),
                        *child_signature,
                    ));
                }
            }
            ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_bitmap_cache_keys(commands, keys);
            }
            ScenePrimitive::CompositingLayer { .. }
            | ScenePrimitive::Rect { .. }
            | ScenePrimitive::Ellipse { .. }
            | ScenePrimitive::Text { .. }
            | ScenePrimitive::Custom { .. }
            | ScenePrimitive::Line { .. }
            | ScenePrimitive::Path { .. }
            | ScenePrimitive::Glow { .. }
            | ScenePrimitive::Overlay { .. } => {}
        }
    }
}

pub(super) fn collect_overlay_brush_cache_keys(
    commands: &[ScenePrimitive],
    keys: &mut HashSet<D2dOverlayBrushCacheKey>,
) {
    for command in commands {
        match command {
            ScenePrimitive::Overlay { rect, style, .. } => {
                keys.insert(overlay_brush_cache_key(*rect, style));
            }
            ScenePrimitive::StaticLayer { spec, commands, .. }
                if spec.cache_policy == RasterCachePolicy::Disabled =>
            {
                collect_overlay_brush_cache_keys(commands, keys);
            }
            ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_overlay_brush_cache_keys(commands, keys);
            }
            _ => {}
        }
    }
}

pub(super) fn pure_static_layer_image(
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
) -> Option<(&'static str, ImageFit)> {
    if !commands.is_empty() || spec.background != StaticLayerBackground::Transparent {
        return None;
    }
    match spec.source {
        StaticLayerSource::BakedAsset { key, fit } => Some((key, fit)),
        StaticLayerSource::Hybrid {
            baked_base: Some(key),
            fit,
        } => Some((key, fit)),
        StaticLayerSource::RuntimeGenerated
        | StaticLayerSource::Hybrid {
            baked_base: None, ..
        } => None,
    }
}

pub(super) fn draw_scene_d2d(
    resources: &mut D2dRenderer,
    list: &Scene,
    clip: Option<UiRect>,
) -> Result<()> {
    draw_commands_d2d(resources, list.commands(), clip)
}

pub(super) fn draw_commands_d2d(
    resources: &mut D2dRenderer,
    commands: &[ScenePrimitive],
    clip: Option<UiRect>,
) -> Result<()> {
    for command in commands {
        if clip
            .and_then(|clip| clip.intersect(command.paint_bounds()))
            .is_none()
            && clip.is_some()
        {
            continue;
        }
        draw_command_d2d(resources, command)?;
    }
    Ok(())
}
