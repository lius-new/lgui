use super::raster::{
    blit_cached_static_layer, render_static_layer_bitmap, store_static_layer_memory, trace_duration,
};
use super::*;

pub fn draw_scroll_raster<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    _child_signature: u64,
) {
    let frame_start = Instant::now();
    for tile_index in &spec.visible_tiles {
        draw_scroll_raster_visible_tile::<B>(
            hdc,
            id,
            viewport,
            spec,
            commands,
            *tile_index,
            _child_signature,
        );
    }

    let mut warmed = 0usize;
    for tile_index in &spec.prefetch_tiles {
        if warmed >= spec.max_prefetch_tiles_per_frame {
            break;
        }
        if frame_start.elapsed().as_millis() >= u128::from(spec.max_prefetch_ms_per_frame) {
            break;
        }
        if scroll_raster_tile_cached(id, viewport, spec, *tile_index, _child_signature) {
            continue;
        }
        if frame_start.elapsed().as_millis() >= u128::from(spec.max_prefetch_ms_per_frame) {
            break;
        }
        if warm_scroll_raster_tile::<B>(
            hdc,
            id,
            viewport,
            spec,
            commands,
            *tile_index,
            _child_signature,
        ) {
            warmed = warmed.saturating_add(1);
        }
    }
}

fn draw_scroll_raster_visible_tile<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    tile_index: usize,
    child_signature: u64,
) {
    let tile_rect = scroll_raster_tile_rect(viewport, spec, tile_index);
    let tile_id = scroll_raster_tile_id(id, tile_index);
    let tile_spec = scroll_raster_tile_spec(spec, 0xFF);
    let key = scroll_raster_tile_cache_key(&tile_id, tile_rect, &tile_spec, spec, child_signature);
    let draw_rect = tile_rect.translate(tile_spec.offset_x, tile_spec.offset_y);
    if blit_cached_static_layer::<B>(hdc, draw_rect, Some(viewport), &key, tile_spec.opacity) {
        return;
    }
    let tile_commands = commands_for_tile(id, spec, commands, tile_rect);
    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, tile_rect, &tile_spec, &tile_commands)
    else {
        return;
    };
    let stored = store_static_layer_memory(
        tile_id.as_str(),
        key.clone(),
        &bitmap,
        tile_spec.memory_budget_bytes,
        tile_spec.cache_policy,
    );
    if stored {
        let _ =
            blit_cached_static_layer::<B>(hdc, draw_rect, Some(viewport), &key, tile_spec.opacity);
    } else if let Some(dest) = draw_rect.intersect(viewport) {
        let source = UiRect::new(
            dest.left - draw_rect.left,
            dest.top - draw_rect.top,
            dest.right - draw_rect.left,
            dest.bottom - draw_rect.top,
        );
        B::blit_cached_premultiplied_bgra_region_alpha(
            hdc,
            &key,
            dest,
            source,
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            tile_spec.opacity,
        );
    }
}

fn warm_scroll_raster_tile<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    tile_index: usize,
    child_signature: u64,
) -> bool {
    let start = Instant::now();
    let tile_rect = scroll_raster_tile_rect(viewport, spec, tile_index);
    let tile_id = scroll_raster_tile_id(id, tile_index);
    let tile_spec = scroll_raster_tile_spec(spec, 0xFF);
    let key = scroll_raster_tile_cache_key(&tile_id, tile_rect, &tile_spec, spec, child_signature);
    if static_layer_memory_contains_key(&key) {
        return false;
    }
    let tile_commands = commands_for_tile(id, spec, commands, tile_rect);
    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, tile_rect, &tile_spec, &tile_commands)
    else {
        return false;
    };
    let stored = store_static_layer_memory(
        tile_id.as_str(),
        key,
        &bitmap,
        tile_spec.memory_budget_bytes,
        tile_spec.cache_policy,
    );
    trace_duration("gdi.scroll_raster.prefetch_generate", start.elapsed());
    stored
}

fn scroll_raster_tile_rect(viewport: UiRect, spec: &ScrollRasterSpec, tile_index: usize) -> UiRect {
    let tile_height = spec.tile_height_px.max(1.0);
    let top = viewport.top + tile_index as f32 * tile_height;
    let bottom = (top + tile_height).min(viewport.top + spec.content_height);
    UiRect::new(viewport.left, top, viewport.right, bottom.max(top + 1.0))
}

fn scroll_raster_tile_id(id: &UiId, tile_index: usize) -> UiId {
    UiId::owned(format!("{}.tile.{tile_index}", id.as_str()))
}

fn scroll_raster_tile_spec(spec: &ScrollRasterSpec, opacity: u8) -> StaticLayerSpec {
    let background = if spec.background_fill.is_some() {
        StaticLayerBackground::Opaque
    } else {
        StaticLayerBackground::Transparent
    };
    StaticLayerSpec::new(StaticLayerSource::runtime())
        .cache_policy(RasterCachePolicy::memory(
            lgui_core::memory::RetentionClass::WhileVisible,
            lgui_core::memory::CachePriority::High,
        ))
        .memory_budget_bytes(spec.memory_budget_bytes)
        .paint_offset(0.0, -spec.scroll_y)
        .opacity(opacity as f32 / 255.0)
        .revision("scroll-raster-height-tile-v1")
        .background(background)
}

fn scroll_raster_tile_cached(
    id: &UiId,
    viewport: UiRect,
    spec: &ScrollRasterSpec,
    tile_index: usize,
    child_signature: u64,
) -> bool {
    let tile_rect = scroll_raster_tile_rect(viewport, spec, tile_index);
    let tile_id = scroll_raster_tile_id(id, tile_index);
    let tile_spec = scroll_raster_tile_spec(spec, 0xFF);
    let key = scroll_raster_tile_cache_key(&tile_id, tile_rect, &tile_spec, spec, child_signature);
    static_layer_memory_contains_key(&key)
}

fn scroll_raster_tile_cache_key(
    id: &UiId,
    rect: UiRect,
    spec: &StaticLayerSpec,
    raster_spec: &ScrollRasterSpec,
    child_signature: u64,
) -> String {
    static_layer_cache_key(
        id,
        spec,
        raster_length(rect.width()),
        raster_length(rect.height()),
        scroll_raster_tile_signature(raster_spec, child_signature),
    )
}

fn scroll_raster_tile_signature(spec: &ScrollRasterSpec, child_signature: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    spec.cache_epoch.hash(&mut hasher);
    spec.tile_height_px.to_bits().hash(&mut hasher);
    spec.content_height.to_bits().hash(&mut hasher);
    spec.background_fill.hash(&mut hasher);
    child_signature.hash(&mut hasher);
    hasher.finish()
}

fn commands_for_tile(
    id: &UiId,
    spec: &ScrollRasterSpec,
    commands: &[ScenePrimitive],
    tile_rect: UiRect,
) -> Vec<ScenePrimitive> {
    let mut tile_commands = Vec::new();
    if let Some(fill) = spec.background_fill {
        tile_commands.push(ScenePrimitive::Rect {
            id: UiId::owned(format!("{}.raster.background", id.as_str())),
            rect: tile_rect,
            style: VisualStyle::filled(fill),
            phase: RenderPhase::Content,
        });
    }
    tile_commands.extend(
        commands
            .iter()
            .filter(|command| command.rect().intersect(tile_rect).is_some())
            .cloned(),
    );
    tile_commands
}
