use super::raster::{
    blit_cached_static_layer, render_static_layer_bitmap, store_static_layer_memory, trace_duration,
};
use super::*;

pub fn draw_static_layer<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    child_signature: u64,
) {
    let start = Instant::now();
    let draw_rect = rect.translate(spec.offset_x, spec.offset_y);
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    let key = static_layer_raster_cache::cache_key(id, spec, width, height, child_signature);
    let use_memory_cache = matches!(
        spec.cache_policy,
        StaticLayerCachePolicy::Memory | StaticLayerCachePolicy::MemoryAndDisk
    );
    let use_disk_cache = matches!(spec.cache_policy, StaticLayerCachePolicy::MemoryAndDisk);

    if commands.is_empty() && !use_memory_cache && !use_disk_cache {
        match &spec.source {
            StaticLayerSource::BakedAsset { key, fit }
            | StaticLayerSource::Hybrid {
                baked_base: Some(key),
                fit,
            } => {
                draw_image(hdc, draw_rect, key, *fit);
                trace_duration("gdi.static_layer.baked", start.elapsed());
                return;
            }
            StaticLayerSource::RuntimeGenerated
            | StaticLayerSource::Hybrid {
                baked_base: None, ..
            } => {}
        }
    }

    if use_memory_cache && blit_cached_static_layer::<B>(hdc, draw_rect, None, &key, spec.opacity) {
        trace_duration("gdi.static_layer.memory_hit", start.elapsed());
        return;
    }

    if use_disk_cache
        && static_layer_raster_cache::load(&key).is_some_and(|raster| {
            let bitmap = StaticLayerBitmap::from(raster);
            store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
            blit_cached_static_layer::<B>(hdc, draw_rect, None, &key, spec.opacity)
        })
    {
        trace_duration("gdi.static_layer.disk_hit", start.elapsed());
        return;
    }

    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, rect, spec, commands) else {
        return;
    };
    if use_disk_cache {
        static_layer_raster_cache::store(&key, &StaticLayerRaster::from(&bitmap));
    }
    if use_memory_cache {
        store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
        let _ = blit_cached_static_layer::<B>(hdc, draw_rect, None, &key, spec.opacity);
    } else {
        B::blit_premultiplied_bgra_alpha(
            hdc,
            draw_rect,
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            spec.opacity,
        );
    }
    trace_duration("gdi.static_layer.generate", start.elapsed());
}

pub fn draw_static_layer_region<B: StaticLayerDrawBackend>(
    hdc: HDC,
    id: &UiId,
    rect: UiRect,
    clip: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
    child_signature: u64,
) -> bool {
    let start = Instant::now();
    let draw_rect = rect.translate(spec.offset_x, spec.offset_y);
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    let key = static_layer_raster_cache::cache_key(id, spec, width, height, child_signature);
    let use_memory_cache = matches!(
        spec.cache_policy,
        StaticLayerCachePolicy::Memory | StaticLayerCachePolicy::MemoryAndDisk
    );
    let use_disk_cache = matches!(spec.cache_policy, StaticLayerCachePolicy::MemoryAndDisk);
    if use_memory_cache
        && blit_cached_static_layer::<B>(hdc, draw_rect, Some(clip), &key, spec.opacity)
    {
        trace_duration("gdi.static_layer.region_memory_hit", start.elapsed());
        return true;
    }

    if use_disk_cache
        && static_layer_raster_cache::load(&key).is_some_and(|raster| {
            let bitmap = StaticLayerBitmap::from(raster);
            store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
            blit_cached_static_layer::<B>(hdc, draw_rect, Some(clip), &key, spec.opacity)
        })
    {
        trace_duration("gdi.static_layer.region_disk_hit", start.elapsed());
        return true;
    }

    let Some(bitmap) = render_static_layer_bitmap::<B>(hdc, rect, spec, commands) else {
        return false;
    };
    if use_disk_cache {
        static_layer_raster_cache::store(&key, &StaticLayerRaster::from(&bitmap));
    }
    if use_memory_cache {
        store_static_layer_memory(id.as_str(), key.clone(), bitmap, spec.memory_budget_bytes);
        let hit = blit_cached_static_layer::<B>(hdc, draw_rect, Some(clip), &key, spec.opacity);
        if hit {
            trace_duration("gdi.static_layer.region_generate", start.elapsed());
        }
        hit
    } else {
        let Some(dest) = draw_rect.intersect(clip) else {
            return true;
        };
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
            spec.opacity,
        );
        trace_duration("gdi.static_layer.region_generate", start.elapsed());
        true
    }
}
