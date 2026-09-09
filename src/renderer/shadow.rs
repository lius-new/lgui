use std::collections::VecDeque;

use crate::core::ShadowStyle;

/// Image completion can change a retained surface without changing scene commands.
pub(crate) fn resolved_content_signature(
    commands: &[crate::core::ScenePrimitive],
    signature: u64,
) -> u64 {
    #[cfg(feature = "images")]
    {
        use crate::core::{ScenePrimitive, UiImageSource};
        use std::hash::{Hash, Hasher};
        fn visit(
            commands: &[ScenePrimitive],
            state: &mut Option<std::collections::hash_map::DefaultHasher>,
            signature: u64,
        ) {
            for command in commands {
                match command {
                    ScenePrimitive::Image {
                        request,
                        source: UiImageSource::Url(_) | UiImageSource::File(_),
                        ..
                    } => {
                        let hasher = state.get_or_insert_with(|| {
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            signature.hash(&mut hasher);
                            hasher
                        });
                        std::mem::discriminant(&crate::assets::request_image(request)).hash(hasher);
                    }
                    ScenePrimitive::CompositingLayer { commands, .. }
                    | ScenePrimitive::StaticLayer { commands, .. }
                    | ScenePrimitive::ScrollRaster { commands, .. }
                    | ScenePrimitive::Clip { commands, .. }
                    | ScenePrimitive::ClipPath { commands, .. } => {
                        visit(commands, state, signature)
                    }
                    _ => {}
                }
            }
        }
        let mut state = None;
        visit(commands, &mut state, signature);
        state.map_or(signature, |hasher| hasher.finish())
    }
    #[cfg(not(feature = "images"))]
    {
        let _ = commands;
        signature
    }
}

/// Composites an alpha-derived shadow behind a padded premultiplied BGRA surface.
/// All backends share this filter so spread, blur and fractional offsets agree.
pub(crate) fn composite_shadow(pixels: &mut [u8], width: usize, height: usize, style: ShadowStyle) {
    if width == 0 || height == 0 || style.alpha == 0 {
        return;
    }
    assert_eq!(pixels.len(), width * height * 4);
    let mut mask: Vec<u8> = pixels.chunks_exact(4).map(|pixel| pixel[3]).collect();
    let radius = style.spread_radius().abs().ceil() as usize;
    if radius > 0 {
        mask = spread_mask(&mask, width, height, radius, style.spread_radius() > 0.0);
    }
    if style.blur_sigma() > 0.0 {
        let image = image::GrayImage::from_raw(width as u32, height as u32, mask)
            .expect("alpha dimensions");
        mask = image::imageops::blur(&image, style.blur_sigma()).into_raw();
    }
    let color = [
        style.color.0 & 255,
        (style.color.0 >> 8) & 255,
        (style.color.0 >> 16) & 255,
    ];
    for y in 0..height {
        for x in 0..width {
            let alpha = sample(
                &mask,
                width,
                height,
                x as f32 - style.offset_x(),
                y as f32 - style.offset_y(),
            );
            let shadow_alpha = (alpha * style.alpha as f32 / 255.0).round() as u32;
            let pixel = &mut pixels[(y * width + x) * 4..][..4];
            let behind = (shadow_alpha * (255 - pixel[3] as u32) + 127) / 255;
            for channel in 0..3 {
                pixel[channel] =
                    (pixel[channel] as u32 + (color[channel] * behind + 127) / 255).min(255) as u8;
            }
            pixel[3] = (pixel[3] as u32 + behind).min(255) as u8;
        }
    }
}

fn sample(mask: &[u8], width: usize, height: usize, x: f32, y: f32) -> f32 {
    let left = x.floor() as isize;
    let top = y.floor() as isize;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let pixel = |x: isize, y: isize| {
        if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
            0.0
        } else {
            mask[y as usize * width + x as usize] as f32
        }
    };
    (pixel(left, top) * (1.0 - fx) + pixel(left + 1, top) * fx) * (1.0 - fy)
        + (pixel(left, top + 1) * (1.0 - fx) + pixel(left + 1, top + 1) * fx) * fy
}

fn spread_mask(mask: &[u8], width: usize, height: usize, radius: usize, dilate: bool) -> Vec<u8> {
    let mut horizontal = vec![0; mask.len()];
    let mut output = vec![0; mask.len()];
    for y in 0..height {
        extrema_line(
            width,
            radius,
            dilate,
            |x| mask[y * width + x],
            |x, value| horizontal[y * width + x] = value,
        );
    }
    for x in 0..width {
        extrema_line(
            height,
            radius,
            dilate,
            |y| horizontal[y * width + x],
            |y, value| output[y * width + x] = value,
        );
    }
    output
}

// Sliding extrema keep morphology linear in the surface size, even for wide spreads.
fn extrema_line(
    length: usize,
    radius: usize,
    dilate: bool,
    read: impl Fn(usize) -> u8,
    mut write: impl FnMut(usize, u8),
) {
    let radius = radius.min(length);
    let mut deque: VecDeque<(usize, u8)> = VecDeque::new();
    for index in 0..length + radius * 2 {
        let value = index
            .checked_sub(radius)
            .filter(|i| *i < length)
            .map(&read)
            .unwrap_or(0);
        while deque.back().is_some_and(|(_, previous)| {
            if dilate {
                *previous <= value
            } else {
                *previous >= value
            }
        }) {
            deque.pop_back();
        }
        deque.push_back((index, value));
        while deque
            .front()
            .is_some_and(|(previous, _)| *previous + radius * 2 < index)
        {
            deque.pop_front();
        }
        if index >= radius * 2 {
            write(
                index - radius * 2,
                deque.front().expect("nonempty window").1,
            );
        }
    }
}

#[cfg(test)]
pub(crate) fn test_scene(alpha: u8, blur: f32) -> crate::core::Scene {
    use crate::core::*;
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

#[cfg(all(
    test,
    any(
        feature = "renderer-skia",
        feature = "advanced-rendering",
        feature = "renderer-d2d"
    )
))]
pub(crate) fn assert_test_pixels(pixels: &[u8], width: usize) {
    let pixel = |x, y| &pixels[(y * width + x) * 4..][..4];
    let near = |a: &[u8], b: &[u8]| {
        assert!(
            a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 2),
            "{a:?} != {b:?}"
        )
    };
    near(pixel(20, 20), &[192, 0, 0, 192]);
    // The shadow uses the combined alpha (192), then its own opacity (128).
    near(pixel(44, 20), &[0, 0, 96, 96]);
    assert_eq!(
        pixel(35, 11)[3],
        0,
        "circle corners must remain transparent"
    );
    assert_eq!(pixel(60, 60)[3], 0);
}

#[cfg(all(test, feature = "images"))]
pub(crate) fn test_shape_scenes() -> Vec<(&'static str, crate::core::Scene)> {
    use crate::core::*;
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

#[cfg(all(test, feature = "images"))]
pub(crate) fn assert_shape_shadow(backend: &str, name: &str, pixels: &[u8]) {
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
            // Place premultiplied pixels on a white inspection background.
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

#[cfg(test)]
#[path = "shadow_test.rs"]
mod tests;
