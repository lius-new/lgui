use super::*;
use crate::core::Color;

#[test]
fn shadow_uses_alpha_not_rgb_or_surface_bounds() {
    let mut pixels = vec![0; 9 * 5 * 4];
    pixels[(2 * 9 + 2) * 4..][..4].copy_from_slice(&[0, 0, 0, 128]);
    pixels[(1 * 9 + 1) * 4..][..4].copy_from_slice(&[255, 255, 255, 0]);
    composite_shadow(
        &mut pixels,
        9,
        5,
        ShadowStyle::new(Color(0xFF0000))
            .alpha(128)
            .blur(0.0)
            .offset(3.0, 0.0),
    );
    assert_eq!(&pixels[(2 * 9 + 5) * 4..][..4], &[0, 0, 64, 64]);
    assert_eq!(pixels[(1 * 9 + 4) * 4 + 3], 0);
    assert_eq!(pixels[(2 * 9 + 2) * 4 + 3], 128);
}

#[test]
fn shadow_blurs_outward_and_preserves_opaque_source() {
    let mut pixels = vec![0; 25 * 25 * 4];
    for y in 10..15 {
        for x in 10..15 {
            pixels[(y * 25 + x) * 4..][..4].copy_from_slice(&[10, 20, 30, 255]);
        }
    }
    composite_shadow(
        &mut pixels,
        25,
        25,
        ShadowStyle::default().offset(0.0, 0.0).blur(2.0),
    );
    assert_eq!(&pixels[(12 * 25 + 12) * 4..][..4], &[10, 20, 30, 255]);
    assert!(pixels[(12 * 25 + 9) * 4 + 3] > 0);
    assert_eq!(pixels[3], 0);
}

#[test]
fn spread_and_erosion_preserve_holes_and_transparency() {
    let mut mask = vec![0; 7 * 7];
    for y in 2..5 {
        for x in 2..5 {
            mask[y * 7 + x] = 128;
        }
    }
    let dilated = spread_mask(&mask, 7, 7, 1, true);
    assert_eq!(dilated.iter().filter(|a| **a == 128).count(), 25);
    let eroded = spread_mask(&mask, 7, 7, 1, false);
    assert_eq!(eroded.iter().filter(|a| **a == 128).count(), 1);
    assert!(spread_mask(&mask, 7, 7, 20, false).iter().all(|a| *a == 0));
}

#[test]
fn fractional_offset_distributes_alpha_without_changing_layout() {
    let mut pixels = vec![0; 8 * 4];
    pixels[3] = 255;
    composite_shadow(
        &mut pixels,
        8,
        1,
        ShadowStyle::default().alpha(255).blur(0.0).offset(2.5, 0.0),
    );
    assert_eq!(pixels[2 * 4 + 3], 128);
    assert_eq!(pixels[3 * 4 + 3], 128);
}
