use super::*;

pub(super) fn render_static_layer_bitmap<B: StaticLayerDrawBackend>(
    hdc: HDC,
    rect: UiRect,
    spec: &StaticLayerSpec,
    commands: &[ScenePrimitive],
) -> Option<StaticLayerBitmap> {
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    B::with_dib_section(hdc, width, height, |memory_dc, bits| {
        B::clear_alpha_buffer(bits, width, height);
        let local_rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
        match &spec.source {
            StaticLayerSource::BakedAsset { key, fit } => {
                draw_image(memory_dc, local_rect, key, *fit);
            }
            StaticLayerSource::RuntimeGenerated => {}
            StaticLayerSource::Hybrid { baked_base, fit } => {
                if let Some(key) = baked_base {
                    draw_image(memory_dc, local_rect, key, *fit);
                }
            }
        }

        for command in commands {
            let local = translate_command(command, -rect.left, -rect.top);
            B::draw_command(memory_dc, &local);
        }
        B::prepare_alpha_buffer(bits, width, height, spec.background);
        let len = (width * height * 4) as usize;
        let pixels = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len) }.to_vec();
        Some(StaticLayerBitmap {
            width,
            height,
            pixels,
        })
    })
    .flatten()
}

impl From<StaticLayerRaster> for StaticLayerBitmap {
    fn from(raster: StaticLayerRaster) -> Self {
        Self {
            width: raster.width,
            height: raster.height,
            pixels: raster.premultiplied_bgra,
        }
    }
}

impl From<&StaticLayerBitmap> for StaticLayerRaster {
    fn from(bitmap: &StaticLayerBitmap) -> Self {
        Self {
            width: bitmap.width,
            height: bitmap.height,
            premultiplied_bgra: bitmap.pixels.clone(),
        }
    }
}

fn translate_command(command: &ScenePrimitive, dx: f32, dy: f32) -> ScenePrimitive {
    lgui::core::translate_scene_primitive_for_backend(command, dx, dy)
}

pub(super) fn blit_cached_static_layer<B: StaticLayerDrawBackend>(
    hdc: HDC,
    rect: UiRect,
    clip: Option<UiRect>,
    key: &str,
    opacity: u8,
) -> bool {
    let mut cache = static_layer_cache()
        .lock()
        .expect("static layer cache poisoned");
    let Some(bitmap) = cache.touch(key) else {
        return false;
    };
    if let Some(clip) = clip {
        let Some(dest) = rect.intersect(clip) else {
            return true;
        };
        let source = UiRect::new(
            dest.left - rect.left,
            dest.top - rect.top,
            dest.right - rect.left,
            dest.bottom - rect.top,
        );
        B::blit_cached_premultiplied_bgra_region_alpha(
            hdc,
            key,
            dest,
            source,
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            opacity,
        );
    } else {
        B::blit_cached_premultiplied_bgra_region_alpha(
            hdc,
            key,
            rect,
            UiRect::new(0.0, 0.0, bitmap.width as f32, bitmap.height as f32),
            bitmap.width,
            bitmap.height,
            &bitmap.pixels,
            opacity,
        );
    }
    true
}

pub(super) fn store_static_layer_memory(
    id: &str,
    key: String,
    bitmap: StaticLayerBitmap,
    budget_bytes: usize,
) {
    static_layer_cache()
        .lock()
        .expect("static layer cache poisoned")
        .store(id, key, bitmap, budget_bytes);
}

pub(super) fn trace_duration(label: &str, duration: Duration) {
    if render_trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}
