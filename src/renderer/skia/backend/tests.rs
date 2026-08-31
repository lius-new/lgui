use super::*;
use crate::{
    assets::{
        AssetBytes, AssetError, AssetResolver, CustomPaintProvider, RenderResources, SceneFragment,
    },
    core::{
        CompositingLayerSpec, CustomPaintStyle, IconStyle, OverlayStyle, Point,
        RadialGradientLayer, RenderPhase, ScenePrimitiveKind, ScrollRasterSpec, StaticLayerSource,
        StaticLayerSpec, UiId, UiPathCommand, UiScale, VerticalGradientLayer,
    },
    renderer::FrameReason,
};

const TEST_CACHE_BUDGET: usize = 96 * 1024 * 1024;

const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5, 0x1C, 0x0C,
    0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xFF, 0x1F, 0x00,
    0x02, 0xEB, 0x01, 0xF5, 0x8F, 0x59, 0x97, 0xDB, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

struct TestAssets;

impl AssetResolver for TestAssets {
    fn resolve(&self, id: &str) -> Result<AssetBytes, AssetError> {
        (id == "test.pixel")
            .then(|| Arc::<[u8]>::from(PIXEL_PNG))
            .ok_or_else(|| AssetError::NotFound(id.to_owned()))
    }
}

struct TestCustomPaint;

impl CustomPaintProvider for TestCustomPaint {
    fn record(
        &self,
        key: &str,
        bounds: UiRect,
        _style: CustomPaintStyle,
    ) -> Result<Option<SceneFragment>, AssetError> {
        Ok((key == "test.custom").then(|| {
            SceneFragment::new(vec![ScenePrimitive::Rect {
                id: test_id("custom.fragment"),
                rect: bounds,
                style: VisualStyle::filled(Color(0x44CC88)),
                phase: RenderPhase::Content,
            }])
        }))
    }
}

fn test_id(name: &str) -> UiId {
    UiId::from_parts(["skia-test", name])
}

fn rect_command(name: &str, rect: UiRect, color: Color) -> ScenePrimitive {
    ScenePrimitive::Rect {
        id: test_id(name),
        rect,
        style: VisualStyle::filled(color),
        phase: RenderPhase::Content,
    }
}

fn triangle(rect: UiRect) -> UiPath {
    UiPath::new([
        UiPathCommand::MoveTo(Point::new(rect.left, rect.bottom)),
        UiPathCommand::LineTo(Point::new(rect.left + rect.width() / 2.0, rect.top)),
        UiPathCommand::LineTo(Point::new(rect.right, rect.bottom)),
        UiPathCommand::Close,
    ])
}

fn draw_scene(
    surface: &mut SkiaSoftwareSurface,
    scene: &Scene,
    full: bool,
    damage: &[PhysicalRect],
) {
    let frame = FrameInfo::new(
        PhysicalRect::new(0, 0, 32, 32),
        damage,
        UiScale::ONE,
        FrameReason::SceneChange,
        full,
    );
    let resources = RenderResources::new()
        .with_resolver(TestAssets)
        .with_custom_paint(TestCustomPaint);
    crate::assets::with_render_resources(resources, || {
        surface.draw(scene, &frame).expect("draw conformance scene")
    });
}

fn pixel(surface: &SkiaSoftwareSurface, x: usize, y: usize) -> [u8; 4] {
    let (width, _) = surface.size();
    let offset = (y * width as usize + x) * 4;
    surface.pixels()[offset..offset + 4].try_into().unwrap()
}

fn primitive_inventory() -> Vec<ScenePrimitive> {
    let rect = UiRect::new(4.0, 4.0, 20.0, 20.0);
    let child = vec![rect_command("child", rect, Color(0x33AAEE))];
    let image_source = UiImageSource::bytes("test.pixel", 1, Arc::new(PIXEL_PNG.to_vec()));
    vec![
        rect_command("rect", rect, Color(0xFF0000)),
        ScenePrimitive::Ellipse {
            id: test_id("ellipse"),
            rect,
            style: VisualStyle::filled(Color(0x00FF00)),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Text {
            id: test_id("text"),
            rect,
            text: "Skia".into(),
            style: TextStyle::new(Color::WHITE, 12.0, 400),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Custom {
            id: test_id("custom"),
            rect,
            key: "test.custom",
            style: Some(CustomPaintStyle::new(Color::WHITE, 1.0)),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Line {
            id: test_id("line"),
            start: Point::new(4.0, 4.0),
            end: Point::new(20.0, 20.0),
            stroke: Stroke::new(Color::WHITE, 2.0, 255),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Path {
            id: test_id("path"),
            rect,
            path: triangle(rect),
            style: PathStyle::filled(Color(0xFFCC00)),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Image {
            id: test_id("image"),
            rect,
            source: image_source.clone(),
            request: crate::core::ImageRequest::new(image_source),
            fit: ImageFit::Fill,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Icon {
            id: test_id("icon"),
            rect,
            key: "copy",
            style: IconStyle::new(Color::WHITE),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Glow {
            id: test_id("glow"),
            rect,
            color: Color(0x33AAFF),
            alpha: 180,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::BackdropBlur {
            id: test_id("blur"),
            rect,
            style: BackdropBlurStyle::new("test.pixel", ImageFit::Fill, rect).radius(2.0),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::BackdropBlurPath {
            id: test_id("blur-path"),
            rect,
            path: triangle(rect),
            style: BackdropBlurStyle::new("test.pixel", ImageFit::Fill, rect).radius(2.0),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Overlay {
            id: test_id("overlay"),
            rect,
            style: OverlayStyle::new()
                .vertical(VerticalGradientLayer::new(Color::WHITE, 0.8, 0.1))
                .radial(RadialGradientLayer::new(
                    Color(0x44AAFF),
                    0.7,
                    0.5,
                    0.5,
                    0.5,
                )),
            phase: RenderPhase::Content,
        },
        ScenePrimitive::CompositingLayer {
            id: test_id("compositing"),
            rect,
            spec: CompositingLayerSpec::new().opacity(0.75),
            commands: child.clone(),
            content_signature: 1,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::StaticLayer {
            id: test_id("static"),
            rect,
            spec: StaticLayerSpec::new(StaticLayerSource::runtime()).transparent_background(),
            commands: child.clone(),
            child_signature: 1,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::ScrollRaster {
            id: test_id("scroll"),
            viewport: rect,
            spec: ScrollRasterSpec {
                cache_epoch: 1,
                content_height: rect.height(),
                scroll_y: 0.0,
                tile_height_px: rect.height(),
                memory_budget_bytes: 1024 * 1024,
                background_fill: None,
                visible_tiles: vec![0],
                prefetch_tiles: Vec::new(),
                max_prefetch_tiles_per_frame: 0,
                max_prefetch_ms_per_frame: 0,
            },
            commands: child.clone(),
            child_signature: 1,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::Clip {
            id: test_id("clip"),
            rect,
            commands: child.clone(),
            child_signature: 1,
            phase: RenderPhase::Content,
        },
        ScenePrimitive::ClipPath {
            id: test_id("clip-path"),
            rect,
            path: triangle(rect),
            commands: child,
            child_signature: 1,
            phase: RenderPhase::Content,
        },
    ]
}

#[test]
fn software_probe_creates_a_real_skia_surface() {
    assert!(probe_skia_support(GraphicsPreference::Software).is_ok());
}

#[test]
fn gpu_probe_reports_only_compiled_drivers() {
    assert_eq!(
        probe_skia_support(GraphicsPreference::OpenGl).is_ok(),
        cfg!(feature = "renderer-skia-gl")
    );
    #[cfg(all(
        feature = "renderer-skia-vulkan",
        any(target_os = "windows", target_os = "linux")
    ))]
    assert_eq!(
        probe_skia_support(GraphicsPreference::Vulkan).is_ok(),
        unsafe { ash::Entry::load() }.is_ok()
    );
    #[cfg(not(all(
        feature = "renderer-skia-vulkan",
        any(target_os = "windows", target_os = "linux")
    )))]
    assert!(probe_skia_support(GraphicsPreference::Vulkan).is_err());
    assert_eq!(
        probe_skia_support(GraphicsPreference::Metal).is_ok(),
        cfg!(all(feature = "renderer-skia-metal", target_os = "macos"))
    );
}

#[test]
fn cache_is_byte_bounded() {
    let mut cache = SkiaCache::new(4 * 4 * 4);
    cache.begin_frame();
    let first = layer_surface(4.0, 4.0).unwrap().image_snapshot();
    cache.insert("first".to_owned(), first);
    cache.begin_frame();
    let second = layer_surface(4.0, 4.0).unwrap().image_snapshot();
    cache.insert("second".to_owned(), second);
    assert!(cache.resident_bytes <= cache.budget_bytes);
    assert_eq!(cache.entries.len(), 1);
}

#[test]
fn raster_policy_evicts_shorter_retention_then_lower_priority() {
    let image = || layer_surface(4.0, 4.0).unwrap().image_snapshot();
    let mut cache = SkiaCache::new(4 * 4 * 4 * 4);
    cache.insert_with_policy(
        "frame-high".to_owned(),
        image(),
        crate::memory::RetentionClass::Frame,
        crate::memory::CachePriority::High,
    );
    cache.insert_with_policy(
        "scene-low".to_owned(),
        image(),
        crate::memory::RetentionClass::Scene,
        crate::memory::CachePriority::Low,
    );
    cache.insert_with_policy(
        "scene-high".to_owned(),
        image(),
        crate::memory::RetentionClass::Scene,
        crate::memory::CachePriority::High,
    );
    cache.insert_with_policy(
        "session-low".to_owned(),
        image(),
        crate::memory::RetentionClass::Session,
        crate::memory::CachePriority::Low,
    );

    cache.set_budget(2 * 4 * 4 * 4);

    assert!(!cache.entries.contains_key("frame-high"));
    assert!(!cache.entries.contains_key("scene-low"));
    assert!(cache.entries.contains_key("scene-high"));
    assert!(cache.entries.contains_key("session-low"));
}

#[test]
fn image_fit_preserves_aspect_ratio() {
    assert_eq!(
        fitted_rect(
            UiRect::new(0.0, 0.0, 100.0, 100.0),
            (200.0, 100.0),
            ImageFit::Contain
        ),
        UiRect::new(0.0, 25.0, 100.0, 75.0)
    );
}

#[test]
fn every_scene_primitive_has_a_real_skia_paint_path() {
    let commands = primitive_inventory();
    let kinds = commands
        .iter()
        .map(ScenePrimitive::kind)
        .collect::<Vec<_>>();
    assert_eq!(kinds, ScenePrimitiveKind::ALL);
    for command in commands {
        let mut scene = Scene::new();
        scene.push(command);
        let mut surface = SkiaSoftwareSurface::new(TEST_CACHE_BUDGET);
        draw_scene(
            &mut surface,
            &scene,
            true,
            &[PhysicalRect::new(0, 0, 32, 32)],
        );
    }
}

#[test]
fn dirty_draw_preserves_pixels_outside_damage_and_clears_removals() {
    let full = [PhysicalRect::new(0, 0, 32, 32)];
    let left = [PhysicalRect::new(0, 0, 16, 32)];
    let mut surface = SkiaSoftwareSurface::new(TEST_CACHE_BUDGET);
    let mut red = Scene::new();
    red.push(rect_command(
        "background-red",
        UiRect::new(0.0, 0.0, 32.0, 32.0),
        Color(0xFF0000),
    ));
    draw_scene(&mut surface, &red, true, &full);
    let red_pixel = pixel(&surface, 24, 16);

    let mut blue = Scene::new();
    blue.push(rect_command(
        "background-blue",
        UiRect::new(0.0, 0.0, 32.0, 32.0),
        Color(0x0000FF),
    ));
    draw_scene(&mut surface, &blue, false, &left);
    assert_ne!(pixel(&surface, 8, 16), red_pixel);
    assert_eq!(pixel(&surface, 24, 16), red_pixel);

    draw_scene(&mut surface, &Scene::new(), false, &left);
    assert_eq!(pixel(&surface, 8, 16), [0, 0, 0, 0]);
    assert_eq!(pixel(&surface, 24, 16), red_pixel);
}

#[test]
fn dpi_projection_and_nested_clip_use_physical_bounds() {
    let mut logical = Scene::new();
    logical.push(ScenePrimitive::Clip {
        id: test_id("outer-clip"),
        rect: UiRect::new(0.0, 0.0, 5.0, 5.0),
        commands: vec![ScenePrimitive::Clip {
            id: test_id("inner-clip"),
            rect: UiRect::new(2.0, 2.0, 5.0, 5.0),
            commands: vec![rect_command(
                "clip-fill",
                UiRect::new(0.0, 0.0, 8.0, 8.0),
                Color::WHITE,
            )],
            child_signature: 1,
            phase: RenderPhase::Content,
        }],
        child_signature: 1,
        phase: RenderPhase::Content,
    });
    let scene = logical.project_to_physical(UiScale::new(2.0));
    let mut surface = SkiaSoftwareSurface::new(TEST_CACHE_BUDGET);
    draw_scene(
        &mut surface,
        &scene,
        true,
        &[PhysicalRect::new(0, 0, 32, 32)],
    );
    assert_eq!(pixel(&surface, 2, 2), [0, 0, 0, 0]);
    assert_ne!(pixel(&surface, 6, 6), [0, 0, 0, 0]);
    assert_eq!(pixel(&surface, 12, 12), [0, 0, 0, 0]);
}

#[test]
fn cache_trim_is_deterministic_at_both_pressure_levels() {
    let mut cache = SkiaCache::new(1024 * 1024);
    cache.begin_frame();
    cache.insert(
        "old".to_owned(),
        layer_surface(4.0, 4.0).unwrap().image_snapshot(),
    );
    for _ in 0..4 {
        cache.begin_frame();
    }
    cache.insert(
        "recent".to_owned(),
        layer_surface(4.0, 4.0).unwrap().image_snapshot(),
    );
    cache.trim(MemoryPressure::Moderate);
    assert!(!cache.entries.contains_key("old"));
    assert!(cache.entries.contains_key("recent"));
    cache.trim(MemoryPressure::Critical);
    assert!(cache.entries.is_empty());
    assert_eq!(cache.resident_bytes, 0);
}

#[test]
fn paragraph_layout_exposes_bidi_carets_selection_and_hit_testing() {
    let request = crate::text::TextLayoutRequest::single_line(
        "abc \u{05d0}\u{05d1}\u{05d2}",
        UiRect::new(0.0, 0.0, 240.0, 32.0),
        -16.0,
        400,
    );
    let layout = crate::text::TextSystem::layout(&SkiaTextSystem, &request).unwrap();
    assert!(layout.width > 0.0);
    assert!(layout.caret_rect(0).is_some());
    assert!(layout.caret_rect(request.text.chars().count()).is_some());
    assert!(layout
        .clusters()
        .iter()
        .any(|cluster| cluster.direction == crate::text::TextDirection::LeftToRight));
    assert!(layout
        .clusters()
        .iter()
        .any(|cluster| cluster.direction == crate::text::TextDirection::RightToLeft));
    assert!(!layout.selection_rects(1..6).is_empty());
    let cluster = layout.clusters().last().unwrap();
    let hit = layout.hit_test(
        (cluster.bounds.left + cluster.bounds.right) * 0.5,
        (cluster.bounds.top + cluster.bounds.bottom) * 0.5,
    );
    assert!(hit.index <= request.text.chars().count());
    assert!(hit.inside);
}

#[test]
fn paragraph_clusters_keep_combining_sequences_together() {
    let request = crate::text::TextLayoutRequest::single_line(
        "a\u{0301}b",
        UiRect::new(0.0, 0.0, 120.0, 32.0),
        -16.0,
        400,
    );
    let layout = crate::text::TextSystem::layout(&SkiaTextSystem, &request).unwrap();
    assert!(layout
        .clusters()
        .iter()
        .any(|cluster| cluster.range == (0..2)));
}

#[test]
fn paragraph_cache_is_reused_reported_and_trimmed() {
    let mut cache = SkiaCache::new(1024 * 1024);
    let mut surface = layer_surface(200.0, 40.0).unwrap();
    let rect = UiRect::new(0.0, 0.0, 200.0, 40.0);
    let style = TextStyle::new(Color::WHITE, -16.0, 400);
    cache.begin_frame();
    cache.draw_text(surface.canvas(), rect, "cached paragraph", style);
    cache.begin_frame();
    cache.draw_text(surface.canvas(), rect, "cached paragraph", style);
    let stats = cache.stats();
    assert_eq!(stats.text_entries, 1);
    assert_eq!(stats.text_misses, 1);
    assert_eq!(stats.text_hits, 1);
    assert!(stats.text_resident_bytes > 0);
    assert!(stats.largest_text_entry_bytes > 0);
    cache.trim(MemoryPressure::Critical);
    assert!(cache.paragraphs.is_empty());
    assert_eq!(cache.resident_bytes, 0);
}

#[test]
fn layer_opacity_and_content_signature_invalidate_retained_images() {
    let bounds = UiRect::new(0.0, 0.0, 16.0, 16.0);
    let layer = |signature, color| ScenePrimitive::CompositingLayer {
        id: test_id("retained-layer"),
        rect: bounds,
        spec: CompositingLayerSpec::new().opacity(0.5),
        commands: vec![rect_command("retained-child", bounds, color)],
        content_signature: signature,
        phase: RenderPhase::Content,
    };
    let damage = [PhysicalRect::new(0, 0, 32, 32)];
    let mut surface = SkiaSoftwareSurface::new(TEST_CACHE_BUDGET);
    let mut first = Scene::new();
    first.push(layer(1, Color(0xFF0000)));
    draw_scene(&mut surface, &first, true, &damage);
    let red = pixel(&surface, 8, 8);
    assert!(red[3] >= 126 && red[3] <= 129);

    let mut second = Scene::new();
    second.push(layer(2, Color(0x0000FF)));
    draw_scene(&mut surface, &second, true, &damage);
    let blue = pixel(&surface, 8, 8);
    assert_ne!(blue, red);
    assert!(surface.cache_stats().misses >= 2);
}
