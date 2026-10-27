use super::*;
use skia_safe::{FontHinting, PixelGeometry, font::Edging};

fn target(mode: TextRasterization, opaque: bool) -> Surface {
    target_with_props(&mode.surface_props(), opaque)
}

fn target_with_props(props: &skia_safe::SurfaceProps, opaque: bool) -> Surface {
    let info = ImageInfo::new(
        (360, 48),
        ColorType::N32,
        if opaque {
            AlphaType::Opaque
        } else {
            AlphaType::Premul
        },
        None,
    );
    surfaces::raster(&info, None, Some(props)).unwrap()
}

#[cfg(target_os = "windows")]
fn pixels(surface: &mut Surface) -> Vec<u8> {
    let info = ImageInfo::new((360, 48), ColorType::N32, AlphaType::Opaque, None);
    let mut pixels = vec![0; 360 * 48 * 4];
    assert!(surface.read_pixels(&info, &mut pixels, 360 * 4, (0, 0)));
    pixels
}

#[cfg(target_os = "windows")]
#[test]
fn dark_lcd_coverage_reduces_ink_without_changing_layout_or_light_themes() {
    for mode in [
        TextRasterization::Grayscale,
        TextRasterization::LcdRgb,
        TextRasterization::LcdBgr,
    ] {
        for background in [Color::BLACK, Color::WHITE] {
            for factor in [0.8, 1.0, 1.25, 1.5] {
                for text in ["configuration abdegopq", "代码编辑器", "00 0689"] {
                    let dark = background == Color::BLACK;
                    for foreground in if dark {
                        [Color::WHITE, Color(0x79c0ff)]
                    } else {
                        [Color::BLACK, Color(0x005cc5)]
                    } {
                        let style = TextStyle::new(foreground, 13.0 * factor, 400)
                            .font_families(&["Cascadia Mono", "Microsoft YaHei"]);
                        let rect = UiRect::new(4.0, 0.0, 356.0, 48.0);
                        let mut cache = SkiaCache::new(1024 * 1024);
                        let mut before = target(mode, true);
                        let mut after =
                            target_with_props(&mode.surface_props_for_background(background), true);
                        for surface in [&mut before, &mut after] {
                            surface.canvas().clear(sk_color(background, 255));
                            cache.draw_text(surface.canvas(), rect, text, style);
                        }
                        let before = pixels(&mut before);
                        let after = pixels(&mut after);
                        // Identical text/style/metrics reuse the shaped paragraph.
                        // Its native glyph strike still responds to canvas gamma.
                        assert_eq!(cache.stats().text_entries, 1);
                        assert_eq!(cache.stats().text_hits, 1);
                        if !dark || mode == TextRasterization::Grayscale {
                            assert_eq!(before, after, "{mode:?}, {text}, {factor}");
                        } else {
                            let energy = |data: &[u8]| -> u64 {
                                data.chunks_exact(4)
                                    .flat_map(|p| &p[..3])
                                    .map(|v| u64::from(*v))
                                    .sum()
                            };
                            assert!(
                                energy(&after) < energy(&before),
                                "{mode:?}, {text}, {factor}"
                            );
                            assert!(after.chunks_exact(4).any(|p| {
                                p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap() > 12
                            }));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn raster_policy_keeps_lcd_order_and_transparent_transform_fallbacks() {
    for mode in [TextRasterization::LcdRgb, TextRasterization::LcdBgr] {
        let mut surface = target(mode, true);
        let canvas = surface.canvas();
        assert_eq!(TextRasterization::for_canvas(canvas, 255), mode);
        assert_eq!(
            TextRasterization::for_canvas(canvas, 180),
            TextRasterization::Grayscale
        );
        canvas.save();
        canvas.scale((1.25, 1.25));
        assert_eq!(
            TextRasterization::for_canvas(canvas, 255),
            TextRasterization::Grayscale
        );
        canvas.restore();
        canvas.save();
        canvas.rotate(10.0, None);
        assert_eq!(
            TextRasterization::for_canvas(canvas, 255),
            TextRasterization::Grayscale
        );
        canvas.restore();
        let mut transparent = target(mode, false);
        assert_eq!(
            TextRasterization::for_canvas(transparent.canvas(), 255),
            TextRasterization::Grayscale
        );
    }
    let mut unknown = target(TextRasterization::Grayscale, true);
    assert_eq!(
        unknown.canvas().props().unwrap().pixel_geometry(),
        PixelGeometry::Unknown
    );
    assert_eq!(
        TextRasterization::for_canvas(unknown.canvas(), 255),
        TextRasterization::Grayscale
    );
    let mut style = SkTextStyle::new();
    TextRasterization::LcdRgb.configure(&mut style);
    assert_eq!(style.font_edging(), Edging::SubpixelAntiAlias);
    assert_eq!(style.font_hinting(), FontHinting::Slight);
    assert!(style.subpixel());
}

#[test]
fn paragraph_cache_separates_gray_rgb_bgr_and_reuses_the_matching_mode() {
    let mut cache = SkiaCache::new(16 * 1024 * 1024);
    let rect = UiRect::new(0.0, 0.0, 360.0, 48.0);
    let style = TextStyle::new(Color::WHITE, 14.0, 400);
    let text = "English 中文 00 0689";
    for mode in [
        TextRasterization::Grayscale,
        TextRasterization::LcdRgb,
        TextRasterization::LcdBgr,
    ] {
        let mut surface = target(mode, true);
        cache.draw_text(surface.canvas(), rect, text, style);
    }
    assert_eq!(cache.stats().text_misses, 3);
    assert_eq!(cache.stats().text_entries, 3);
    let mut surface = target(TextRasterization::LcdRgb, true);
    cache.draw_text(surface.canvas(), rect, text, style);
    assert_eq!(cache.stats().text_hits, 1);
    let mut transparent = target(TextRasterization::LcdRgb, false);
    cache.draw_text(transparent.canvas(), rect, text, style);
    assert_eq!(cache.stats().text_hits, 2);
    assert_eq!(cache.stats().text_entries, 3);
}

#[cfg(target_os = "windows")]
#[test]
fn lcd_covers_english_chinese_and_digits_at_fractional_physical_sizes() {
    for factor in [0.8, 1.0, 1.25, 1.5, 2.0] {
        for text in ["Native API started", "代码编辑器", "00 0689"] {
            for mode in [
                TextRasterization::Grayscale,
                TextRasterization::LcdRgb,
                TextRasterization::LcdBgr,
            ] {
                let mut surface = target(mode, true);
                surface.canvas().clear(SkColor::BLACK);
                let mut cache = SkiaCache::new(1024 * 1024);
                // DPI changes the physical font size before shaping, never scales
                // an already rasterized text image or the destination canvas.
                let style = TextStyle::new(Color::WHITE, 12.0 * factor, 400)
                    .font_families(&["Consolas", "Microsoft YaHei"]);
                cache.draw_text(
                    surface.canvas(),
                    UiRect::new(4.0, 0.0, 356.0, 48.0),
                    text,
                    style,
                );
                let info = ImageInfo::new((360, 48), ColorType::N32, AlphaType::Opaque, None);
                let mut pixels = vec![0; 360 * 48 * 4];
                assert!(surface.read_pixels(&info, &mut pixels, 360 * 4, (0, 0)));
                let colored = pixels
                    .chunks_exact(4)
                    .filter(|p| p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap() > 12)
                    .count();
                if mode == TextRasterization::Grayscale {
                    assert_eq!(colored, 0);
                } else {
                    assert!(
                        colored > 0,
                        "missing LCD coverage for {text}, {factor}, {mode:?}"
                    );
                }
            }
        }
    }
}
