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
#[path = "shadow_test.rs"]
mod tests;
