use super::*;

fn shadow_test_renderer() -> D2dRenderer {
    use windows::Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct2D::{
                D2D1CreateFactory, ID2D1Factory1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
            },
            Direct3D::D3D_DRIVER_TYPE_WARP,
            Direct3D11::{D3D11CreateDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION},
            DirectWrite::{DWriteCreateFactory, DWRITE_FACTORY_TYPE_SHARED},
            Dxgi::IDXGIDevice,
        },
    };
    unsafe {
        let mut device = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .unwrap();
        let dxgi: IDXGIDevice = device.unwrap().cast().unwrap();
        let factory: ID2D1Factory1 =
            D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap();
        let device = factory.CreateDevice(&dxgi).unwrap();
        let context = device
            .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
            .unwrap();
        let dwrite = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap();
        D2dRenderer::new(context, dwrite, 64, 64).unwrap()
    }
}

#[test]
fn d2d_shadow_uses_subtree_alpha_and_refreshes_cached_pixels() {
    let _gdiplus = crate::gdiplus::GdiPlusRuntime::start().unwrap();
    use lgui_render_api::test_support::{
        assert_shadow_pixels, assert_shape_shadow, shadow_scene, shape_shadow_scenes,
    };
    let mut renderer = shadow_test_renderer();
    let scene = shadow_scene(128, 0.0);
    renderer.draw_scene_full(&scene).unwrap();
    let snapshot = read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap();
    assert_shadow_pixels(&snapshot, 64);
    renderer.draw_scene_full(&scene).unwrap();
    assert_eq!(
        snapshot,
        read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap()
    );
    renderer.draw_scene_full(&shadow_scene(128, 2.0)).unwrap();
    let pixels = read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap();
    assert!(pixels[(9 * 64 + 44) * 4 + 3] > 0);
    renderer.draw_scene_full(&shadow_scene(0, 2.0)).unwrap();
    let pixels = read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap();
    assert_eq!(&pixels[(20 * 64 + 44) * 4..][..4], &[0; 4]);
    for (name, scene) in shape_shadow_scenes() {
        renderer.draw_scene_full(&scene).unwrap();
        let pixels = read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap();
        assert_shape_shadow("d2d", name, &pixels);
    }
}

#[test]
fn d2d_path_clip_preserves_shadow_content_and_clips_its_overflow() {
    use lgui_core::core::{Point, RenderPhase};
    let mut renderer = shadow_test_renderer();
    let source = lgui_render_api::test_support::shadow_scene(128, 0.0);
    let mut scene = Scene::new();
    scene.push(ScenePrimitive::ClipPath {
        id: UiId::new("shadow-clip"),
        rect: UiRect::new(0.0, 0.0, 64.0, 64.0),
        path: UiPath::new([
            UiPathCommand::MoveTo(Point::new(0.0, 0.0)),
            UiPathCommand::LineTo(Point::new(48.0, 0.0)),
            UiPathCommand::LineTo(Point::new(48.0, 64.0)),
            UiPathCommand::LineTo(Point::new(0.0, 64.0)),
            UiPathCommand::Close,
        ]),
        commands: source.commands().to_vec(),
        child_signature: 1,
        phase: RenderPhase::Content,
    });
    renderer.draw_scene_full(&scene).unwrap();
    let pixels = read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap();
    lgui_render_api::test_support::assert_shadow_pixels(&pixels, 64);
    assert_eq!(&pixels[(20 * 64 + 50) * 4..][..4], &[0; 4]);

    renderer.trim_to(0);
    renderer
        .draw_scene_dirty(&scene, &[UiRect::new(0.0, 0.0, 64.0, 64.0)])
        .unwrap();
    assert_eq!(
        pixels,
        read_bitmap_bgra(&renderer.context, &renderer.scene_bitmap, 64, 64).unwrap()
    );
}

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
fn bitmap_cache_budget_evicts_oldest_entries_and_rejects_oversized_entries() {
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
        (
            first.clone(),
            lgui_core::memory::RetentionClass::Frame,
            lgui_core::memory::CachePriority::High,
            1,
            first.estimated_bytes(),
        ),
        (
            second.clone(),
            lgui_core::memory::RetentionClass::Scene,
            lgui_core::memory::CachePriority::Low,
            2,
            second.estimated_bytes(),
        ),
        (
            third.clone(),
            lgui_core::memory::RetentionClass::Scene,
            lgui_core::memory::CachePriority::High,
            3,
            third.estimated_bytes(),
        ),
    ];
    let total = entries.iter().map(|(_, _, _, _, bytes)| bytes).sum();

    let evictions = bitmap_cache_eviction_plan(entries, total, third.estimated_bytes());

    assert_eq!(evictions, vec![first, second]);
    assert_eq!(
        bitmap_cache_eviction_plan(
            vec![(
                third.clone(),
                lgui_core::memory::RetentionClass::Session,
                lgui_core::memory::CachePriority::High,
                1,
                third.estimated_bytes(),
            )],
            third.estimated_bytes(),
            1,
        ),
        vec![third]
    );
}

#[test]
fn native_radial_gradient_stops_follow_the_quadratic_falloff() {
    let stops = radial_gradient_stops(lgui_core::core::RadialGradientLayer::new(
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
        .vertical(lgui_core::core::VerticalGradientLayer::new(
            Color(0x112233),
            0.1,
            0.4,
        ))
        .radial(lgui_core::core::RadialGradientLayer::new(
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
        phase: lgui_core::core::RenderPhase::Content,
    };

    let keys = overlay_brush_cache_keys(&[overlay(rect, style.clone())]);
    assert_eq!(keys.len(), 1);
    assert!(keys.contains(&overlay_brush_cache_key(rect, &style)));

    let moved_keys = overlay_brush_cache_keys(&[overlay(rect.translate(10.0, 0.0), style.clone())]);
    let changed_style = style.radial(lgui_core::core::RadialGradientLayer::new(
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
fn path_blur_cache_reachability_tracks_path_but_reuses_opacity() {
    let rect = UiRect::new(0.0, 0.0, 320.0, 180.0);
    let style = lgui_core::core::BackdropBlurStyle::new(
        "background",
        ImageFit::Cover,
        UiRect::new(0.0, 0.0, 640.0, 360.0),
    )
    .radius(20.0)
    .opacity(0.4);
    let path = UiPath::new([
        UiPathCommand::MoveTo(lgui_core::core::Point::new(0.0, 0.0)),
        UiPathCommand::LineTo(lgui_core::core::Point::new(320.0, 0.0)),
        UiPathCommand::LineTo(lgui_core::core::Point::new(320.0, 180.0)),
        UiPathCommand::Close,
    ]);
    let changed_path = UiPath::new([
        UiPathCommand::MoveTo(lgui_core::core::Point::new(0.0, 0.0)),
        UiPathCommand::LineTo(lgui_core::core::Point::new(280.0, 0.0)),
        UiPathCommand::LineTo(lgui_core::core::Point::new(320.0, 180.0)),
        UiPathCommand::Close,
    ]);
    let key = backdrop_blur_path_cache_key(rect, &path, style);

    assert_eq!(
        key,
        backdrop_blur_path_cache_key(rect, &path, style.opacity(0.9))
    );
    assert_ne!(
        key,
        backdrop_blur_path_cache_key(rect, &changed_path, style)
    );

    let keys = bitmap_cache_keys(&[ScenePrimitive::BackdropBlurPath {
        id: UiId::owned("path-blur".to_string()),
        rect,
        path,
        style,
        phase: lgui_core::core::RenderPhase::Content,
    }]);
    assert_eq!(keys.len(), 1);
    assert!(keys.contains(&key));
}

#[test]
fn transparent_pure_image_static_layer_reuses_the_image_cache_key() {
    let rect = UiRect::new(0.0, 0.0, 320.0, 180.0);
    let spec = StaticLayerSpec::new(StaticLayerSource::hybrid(
        Some("background"),
        ImageFit::Cover,
    ))
    .cache_policy(RasterCachePolicy::memory(
        lgui_core::memory::RetentionClass::Scene,
        lgui_core::memory::CachePriority::Normal,
    ))
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
            phase: lgui_core::core::RenderPhase::Content,
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
        request: lgui_core::core::ImageRequest::new(UiImageSource::Static(name)),
        fit: ImageFit::Cover,
        phase: lgui_core::core::RenderPhase::Content,
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
