use super::*;

#[test]
fn transparent_surface_reconstruction_preserves_black_and_partial_alpha() {
    assert_eq!(
        synthesize_transparent_pixel(&[0, 0, 0, 255], &[127, 127, 127, 255]),
        [0, 0, 0, 128]
    );
    assert_eq!(
        synthesize_transparent_pixel(&[0, 0, 128, 255], &[127, 127, 255, 255]),
        [0, 0, 128, 128]
    );
}

#[test]
fn transparent_surface_reconstruction_handles_clear_and_opaque_pixels() {
    assert_eq!(
        synthesize_transparent_pixel(&[0, 0, 0, 255], &[255, 255, 255, 255]),
        [0, 0, 0, 0]
    );
    assert_eq!(
        synthesize_transparent_pixel(&[10, 20, 30, 255], &[10, 20, 30, 255]),
        [10, 20, 30, 255]
    );
}

#[test]
fn transformed_destination_points_follow_layer_origin() {
    let rect = UiRect::new(10.0, 20.0, 30.0, 60.0);
    let transform = LayerTransform::identity()
        .scale_xy(2.0, 1.0)
        .rotation_degrees(90.0)
        .translation(75.0, -25.0)
        .origin(0.5, 0.5);
    let points = gdi_layer_destination_points(rect, transform);
    let expected = [
        transform.transform_point(rect, 10.0, 20.0),
        transform.transform_point(rect, 30.0, 20.0),
        transform.transform_point(rect, 10.0, 60.0),
    ];
    for (point, expected) in points.iter().zip(expected) {
        assert!((point.X - expected.0).abs() < 0.001);
        assert!((point.Y - expected.1).abs() < 0.001);
    }
}

#[test]
fn gdi_bitmap_cache_probes_an_existing_entry_without_source_pixels() {
    let seed = unsafe { CreateCompatibleDC(None) };
    assert!(!seed.is_invalid());
    let mut cache = GdiBitmapCache::default();
    cache.budget_bytes = 4096;
    let pixels = vec![255; 8 * 8 * 4];

    assert!(cache.entry(seed, "blur", 8, 8, &pixels).is_some());
    let misses = cache.misses;
    assert!(cache.existing_entry("blur", 8, 8).is_some());
    assert_eq!(cache.hits, 1);
    assert_eq!(cache.misses, misses);
    assert!(cache.existing_entry("blur", 9, 8).is_none());
    assert_eq!(cache.misses, misses);

    drop(cache);
    unsafe {
        let _ = DeleteDC(seed);
    }
}

#[test]
fn transparent_gdi_layer_updates_only_changed_black_content() {
    let seed = unsafe { CreateCompatibleDC(None) };
    assert!(!seed.is_invalid());
    let mut layer = GdiCompositingLayer::new(seed, 32, 32, CompositingLayerBackground::Transparent)
        .expect("create test layer");
    let line = |id_value: &str, y: f32| ScenePrimitive::Line {
        id: UiId::owned(id_value.to_string()),
        start: Point::new(4.0, y),
        end: Point::new(24.0, y),
        stroke: Stroke::new(Color::BLACK, 1.0, 255),
        phase: lgui::core::RenderPhase::Content,
    };
    let previous = vec![line("black-line", 6.0)];
    layer.redraw(&previous, &[UiRect::new(0.0, 0.0, 32.0, 32.0)]);
    assert_eq!(surface_pixel(&layer.output, 12, 6), [0, 0, 0, 255]);
    assert_eq!(surface_pixel(&layer.output, 12, 20), [0, 0, 0, 0]);

    let next = vec![line("black-line", 20.0)];
    let damage = compositing_layer_damage(
        &previous,
        &next,
        UiRect::new(0.0, 0.0, layer.width as f32, layer.height as f32),
    );
    layer.redraw(&next, &damage);
    assert_eq!(surface_pixel(&layer.output, 12, 6), [0, 0, 0, 0]);
    assert_eq!(surface_pixel(&layer.output, 12, 20), [0, 0, 0, 255]);

    unsafe {
        let _ = DeleteDC(seed);
    }
}

fn surface_pixel(surface: &GdiBitmapEntry, x: i32, y: i32) -> [u8; 4] {
    let offset = ((y * surface.width + x) * 4) as usize;
    unsafe {
        let pixel = std::slice::from_raw_parts(surface.bits.add(offset), 4);
        [pixel[0], pixel[1], pixel[2], pixel[3]]
    }
}
