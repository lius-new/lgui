//! Shared renderer conformance fixtures.

use lgui_core::core::*;

pub fn shadow_scene(alpha: u8, blur: f32) -> Scene {
    let mut tree = HostTree::new();
    tree.push(
        UiNode::new(
            UiId::new("shadow-test"),
            UiNodeKind::Group,
            UiRect::new(8.0, 8.0, 32.0, 32.0),
        )
        .shadow(
            ShadowStyle::new(Color(0xFF0000))
                .alpha(128)
                .blur(blur)
                .offset(24.0, 0.0),
        ),
    );
    for name in ["circle-a", "circle-b"] {
        tree.push(
            UiNode::new(
                UiId::new(name),
                UiNodeKind::Ellipse,
                UiRect::new(10.0, 10.0, 30.0, 30.0),
            )
            .parent(UiId::new("shadow-test"))
            .style(VisualStyle::filled(Color(0x0000FF)).alpha(alpha)),
        );
    }
    tree.scene()
}

pub fn assert_shadow_pixels(pixels: &[u8], width: usize) {
    let pixel = |x, y| &pixels[(y * width + x) * 4..][..4];
    let near = |a: &[u8], b: &[u8]| {
        assert!(
            a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 2),
            "{a:?} != {b:?}"
        )
    };
    near(pixel(20, 20), &[192, 0, 0, 192]);
    near(pixel(44, 20), &[0, 0, 96, 96]);
    assert_eq!(
        pixel(35, 11)[3],
        0,
        "circle corners must remain transparent"
    );
    assert_eq!(pixel(60, 60)[3], 0);
}

pub fn shape_shadow_scenes() -> Vec<(&'static str, Scene)> {
    let bounds = UiRect::new(8.0, 8.0, 24.0, 24.0);
    let node = |name, kind| UiNode::new(UiId::new(name), kind, bounds);
    let fill = VisualStyle::filled(Color(0x2378B0));
    let triangle = UiPath::new([
        UiPathCommand::MoveTo(Point::new(8.0, 24.0)),
        UiPathCommand::LineTo(Point::new(16.0, 8.0)),
        UiPathCommand::LineTo(Point::new(24.0, 24.0)),
        UiPathCommand::Close,
    ]);
    let image = image::RgbaImage::from_fn(16, 16, |x, y| {
        image::Rgba(if x < 4 || y < 4 {
            [35, 120, 176, 255]
        } else {
            [0; 4]
        })
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    [
        (
            "rectangle",
            node("rectangle", UiNodeKind::Panel).style(fill),
        ),
        (
            "rounded",
            node("rounded", UiNodeKind::Panel).style(fill.radius(5.0)),
        ),
        ("circle", node("circle", UiNodeKind::Ellipse).style(fill)),
        (
            "path",
            node("path", UiNodeKind::Path).path(triangle, PathStyle::filled(Color(0x2378B0))),
        ),
        (
            "glyph",
            node("glyph", UiNodeKind::Text).text("O", TextStyle::new(Color(0x2378B0), 16.0, 400)),
        ),
        (
            "image",
            node("image", UiNodeKind::Image).image(
                UiImageSource::bytes("shadow-transparent-image", 1, encoded.into_inner()),
                ImageFit::Fill,
            ),
        ),
    ]
    .into_iter()
    .map(|(name, node)| {
        let mut tree = HostTree::new();
        tree.push(
            node.shadow(
                ShadowStyle::new(Color::BLACK)
                    .alpha(255)
                    .blur(0.0)
                    .offset(32.0, 0.0),
            ),
        );
        (name, tree.scene())
    })
    .collect()
}

pub fn assert_shape_shadow(backend: &str, name: &str, pixels: &[u8]) {
    let mut covered = 0;
    let mut clear = 0;
    for y in 0..32 {
        for x in 0..32 {
            let source = pixels[(y * 64 + x) * 4 + 3];
            let shadow = pixels[(y * 64 + x + 32) * 4 + 3];
            assert!(
                source.abs_diff(shadow) <= 2,
                "{backend} {name} alpha at {x},{y}: {source} != {shadow}"
            );
            covered += usize::from(source > 0);
            clear += usize::from(source == 0);
        }
    }
    assert!(
        covered > 5 && clear > 5,
        "{backend} {name} must render a nonempty silhouette"
    );
    if let Ok(directory) = std::env::var("LGUI_SHADOW_ARTIFACT_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        let mut rgba = pixels.to_vec();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            let white = 255 - pixel[3];
            for channel in &mut pixel[..3] {
                *channel = channel.saturating_add(white);
            }
            pixel[3] = 255;
        }
        let path = std::path::Path::new(&directory).join(format!("{backend}-{name}.png"));
        image::save_buffer(path, &rgba[..64 * 32 * 4], 64, 32, image::ColorType::Rgba8).unwrap();
    }
}
