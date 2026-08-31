use super::*;

#[test]
fn d2d_matrix_matches_backend_neutral_layer_transform() {
    let rect = UiRect::new(10.0, 20.0, 30.0, 60.0);
    let transform = LayerTransform::identity()
        .scale_xy(2.0, 1.0)
        .rotation_degrees(90.0)
        .translation(75.0, -25.0)
        .origin(0.5, 0.5);
    let matrix = d2d_layer_transform(rect, transform);
    let apply = |x: f32, y: f32| {
        (
            x * matrix.M11 + y * matrix.M21 + matrix.M31,
            x * matrix.M12 + y * matrix.M22 + matrix.M32,
        )
    };
    let actual = apply(rect.left as f32, rect.top as f32);
    let expected = transform.transform_point(rect, rect.left as f32, rect.top as f32);
    assert!((actual.0 - expected.0).abs() < 0.001);
    assert!((actual.1 - expected.1).abs() < 0.001);
}

#[test]
fn bitmap_cache_budget_evicts_oldest_entries_and_keeps_one_oversized_entry() {
    let first = image_cache_key(
        UiRect::new(0.0, 0.0, 10.0, 10.0),
        &UiImageSource::Static("first"),
        ImageFit::Fill,
    );
    let second = image_cache_key(
        UiRect::new(0.0, 0.0, 20.0, 10.0),
        &UiImageSource::Static("second"),
        ImageFit::Fill,
    );
    let third = image_cache_key(
        UiRect::new(0.0, 0.0, 30.0, 10.0),
        &UiImageSource::Static("third"),
        ImageFit::Fill,
    );
    let entries = vec![
        (first.clone(), 1, first.estimated_bytes()),
        (second.clone(), 2, second.estimated_bytes()),
        (third.clone(), 3, third.estimated_bytes()),
    ];
    let total = entries.iter().map(|(_, _, bytes)| bytes).sum();

    let evictions = bitmap_cache_eviction_plan(entries, total, third.estimated_bytes());

    assert_eq!(evictions, vec![first, second]);
    assert!(bitmap_cache_eviction_plan(
        vec![(third.clone(), 1, third.estimated_bytes())],
        third.estimated_bytes(),
        1,
    )
    .is_empty());
    assert_eq!(
        d2d_bitmap_cache_budget(1432, 860),
        D2D_BITMAP_CACHE_MIN_BUDGET_BYTES
    );
    assert!(d2d_bitmap_cache_budget(3840, 2160) > D2D_BITMAP_CACHE_MIN_BUDGET_BYTES);
}

#[test]
fn native_radial_gradient_stops_follow_the_quadratic_falloff() {
    let stops = radial_gradient_stops(lgui::core::RadialGradientLayer::new(
        Color(0x336699),
        0.8,
        0.5,
        0.5,
        0.5,
    ));

    assert_eq!(stops.map(|stop| stop.position), [0.0, 0.25, 0.5, 0.75, 1.0]);
    assert!((stops[0].color.a - 0.8).abs() < 0.0001);
    assert!((stops[2].color.a - 0.2).abs() < 0.0001);
    assert_eq!(stops[4].color.a, 0.0);
}

#[test]
fn overlay_brush_cache_reachability_tracks_style_and_rect() {
    let rect = UiRect::new(0.0, 0.0, 320.0, 180.0);
    let style = OverlayStyle::new()
        .vertical(lgui::core::VerticalGradientLayer::new(
            Color(0x112233),
            0.1,
            0.4,
        ))
        .radial(lgui::core::RadialGradientLayer::new(
            Color(0x445566),
            0.2,
            0.5,
            0.5,
            0.6,
        ));
    let overlay = |rect, style| ScenePrimitive::Overlay {
        id: UiId::owned("overlay".to_string()),
        rect,
        style,
        phase: lgui::core::RenderPhase::Content,
    };

    let keys = overlay_brush_cache_keys(&[overlay(rect, style.clone())]);
    assert_eq!(keys.len(), 1);
    assert!(keys.contains(&overlay_brush_cache_key(rect, &style)));

    let moved_keys = overlay_brush_cache_keys(&[overlay(rect.translate(10.0, 0.0), style.clone())]);
    let changed_style = style.radial(lgui::core::RadialGradientLayer::new(
        Color(0x778899),
        0.3,
        0.4,
        0.4,
        0.5,
    ));
    let changed_keys = overlay_brush_cache_keys(&[overlay(rect, changed_style)]);

    assert!(keys.is_disjoint(&moved_keys));
    assert!(keys.is_disjoint(&changed_keys));
}

#[test]
fn transparent_pure_image_static_layer_reuses_the_image_cache_key() {
    let rect = UiRect::new(0.0, 0.0, 320.0, 180.0);
    let spec = StaticLayerSpec::new(StaticLayerSource::hybrid(
        Some("background"),
        ImageFit::Cover,
    ))
    .cache_policy(StaticLayerCachePolicy::Memory)
    .transparent_background();
    assert_eq!(
        pure_static_layer_image(&spec, &[]),
        Some(("background", ImageFit::Cover))
    );

    let mut keys = HashSet::new();
    collect_bitmap_cache_keys(
        &[ScenePrimitive::StaticLayer {
            id: UiId::owned("static".to_string()),
            rect,
            spec: spec.clone(),
            commands: Vec::new(),
            child_signature: 7,
            phase: lgui::core::RenderPhase::Content,
        }],
        &mut keys,
    );

    assert_eq!(keys.len(), 1);
    assert!(keys.contains(&image_cache_key(
        UiRect::new(0.0, 0.0, rect.width(), rect.height()),
        &UiImageSource::Static("background"),
        ImageFit::Cover,
    )));
    assert!(pure_static_layer_image(
        &StaticLayerSpec::new(StaticLayerSource::baked("background", ImageFit::Cover)),
        &[],
    )
    .is_none());
}

#[test]
fn bitmap_cache_reachability_replaces_keys_from_the_previous_scene() {
    let image = |name| ScenePrimitive::Image {
        id: UiId::owned(format!("{name}-image")),
        rect: UiRect::new(0.0, 0.0, 64.0, 64.0),
        source: UiImageSource::Static(name),
        fit: ImageFit::Cover,
        phase: lgui::core::RenderPhase::Content,
    };

    let login_keys = bitmap_cache_keys(&[image("login")]);
    let lobby_keys = bitmap_cache_keys(&[image("lobby")]);

    assert_eq!(login_keys.len(), 1);
    assert_eq!(lobby_keys.len(), 1);
    assert!(login_keys.is_disjoint(&lobby_keys));
    assert!(lobby_keys.contains(&image_cache_key(
        UiRect::new(0.0, 0.0, 64.0, 64.0),
        &UiImageSource::Static("lobby"),
        ImageFit::Cover,
    )));
}
