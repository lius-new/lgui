use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
    sync::OnceLock,
    time::Instant,
};

use lgui::core::{BackdropBlurStyle, PhysicalRect, UiRect};
use lgui::platform::win32::render_trace;

use super::image;

const DEFAULT_BLUR_RESULT_BUDGET: usize = 12 * 1024 * 1024;
const DEFAULT_BLUR_SOURCE_BUDGET: usize = 8 * 1024 * 1024;
const DEFAULT_BLURRED_SOURCE_BUDGET: usize = 12 * 1024 * 1024;

fn blur_telemetry(index: usize) -> &'static crate::memory::CacheTelemetry {
    static TELEMETRY: OnceLock<[crate::memory::CacheTelemetry; 3]> = OnceLock::new();
    &TELEMETRY.get_or_init(Default::default)[index]
}

thread_local! {
    static BLUR_CACHE: RefCell<crate::memory::LruCache<BlurCacheKey, Vec<u8>>> = RefCell::new(
        crate::memory::LruCache::new(
            DEFAULT_BLUR_RESULT_BUDGET,
            crate::memory::ResourceClass::Cache,
            blur_telemetry(0).clone(),
        )
    );
    static SOURCE_RASTER_CACHE: RefCell<crate::memory::LruCache<SourceRasterKey, image::RasterImage>> = RefCell::new(
        crate::memory::LruCache::new(
            DEFAULT_BLUR_SOURCE_BUDGET,
            crate::memory::ResourceClass::Cache,
            blur_telemetry(1).clone(),
        )
    );
    static BLURRED_SOURCE_CACHE: RefCell<crate::memory::LruCache<BlurredSourceKey, image::RasterImage>> = RefCell::new(
        crate::memory::LruCache::new(
            DEFAULT_BLURRED_SOURCE_BUDGET,
            crate::memory::ResourceClass::Cache,
            blur_telemetry(2).clone(),
        )
    );
}

pub(crate) fn blur_cache_usage() -> crate::memory::CacheUsage {
    let mut usage = crate::memory::CacheUsage::default();
    for index in 0..3 {
        usage.add_assign(blur_telemetry(index).snapshot());
    }
    usage
}

pub(crate) fn trim_blur_caches(target_bytes: usize) -> usize {
    let result_target = target_bytes.saturating_mul(3) / 8;
    let source_target = target_bytes / 4;
    let blurred_target = target_bytes.saturating_sub(result_target + source_target);
    BLUR_CACHE.with(|cache| cache.borrow_mut().trim_to(result_target))
        + SOURCE_RASTER_CACHE.with(|cache| cache.borrow_mut().trim_to(source_target))
        + BLURRED_SOURCE_CACHE.with(|cache| cache.borrow_mut().trim_to(blurred_target))
}

pub(crate) fn set_blur_cache_budget(budget_bytes: usize) {
    let result_budget = budget_bytes.saturating_mul(3) / 8;
    let source_budget = budget_bytes / 4;
    let blurred_budget = budget_bytes.saturating_sub(result_budget + source_budget);
    BLUR_CACHE.with(|cache| cache.borrow_mut().set_budget(result_budget));
    SOURCE_RASTER_CACHE.with(|cache| cache.borrow_mut().set_budget(source_budget));
    BLURRED_SOURCE_CACHE.with(|cache| cache.borrow_mut().set_budget(blurred_budget));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceRasterKey {
    source: &'static str,
    fit: lgui::core::ImageFit,
    source_rect: UiRect,
}

impl Hash for SourceRasterKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.fit.hash(state);
        hash_rect(&self.source_rect, state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlurredSourceKey {
    source: &'static str,
    fit: lgui::core::ImageFit,
    source_rect: UiRect,
    radius: usize,
    tint: u32,
    tint_alpha_bits: u32,
}

impl Hash for BlurredSourceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.fit.hash(state);
        hash_rect(&self.source_rect, state);
        self.radius.hash(state);
        self.tint.hash(state);
        self.tint_alpha_bits.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlurCacheKey {
    source: &'static str,
    fit: lgui::core::ImageFit,
    source_rect: UiRect,
    sample_rect: UiRect,
    radius: usize,
    tint: u32,
    tint_alpha_bits: u32,
}

impl Hash for BlurCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.fit.hash(state);
        hash_rect(&self.source_rect, state);
        hash_rect(&self.sample_rect, state);
        self.radius.hash(state);
        self.tint.hash(state);
        self.tint_alpha_bits.hash(state);
    }
}

pub fn with_backdrop_blur_bgra<T>(
    rect: UiRect,
    style: BackdropBlurStyle,
    draw: impl FnOnce(&[u8], i32, i32, f32) -> T,
) -> Option<T> {
    let total_start = Instant::now();
    let width = rect.width().ceil().max(1.0) as i32;
    let height = rect.height().ceil().max(1.0) as i32;
    if width <= 0 || height <= 0 {
        return None;
    }

    let source_rect = style.source_rect;
    let sample_rect = UiRect::new(
        rect.left - source_rect.left,
        rect.top - source_rect.top,
        rect.right - source_rect.left,
        rect.bottom - source_rect.top,
    );
    let radius = style.radius.ceil().max(0.0) as usize;
    let key = BlurCacheKey {
        source: style.source,
        fit: style.fit,
        source_rect,
        sample_rect,
        radius,
        tint: style.tint.0,
        tint_alpha_bits: style.tint_alpha.to_bits(),
    };

    BLUR_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_touch(&key) {
            trace_blur_miss(rect, width, height, radius);
            let blurred =
                with_blurred_source(style.source, source_rect, style.fit, style, |source| {
                    let crop_start = Instant::now();
                    let blurred = crop_bgra(
                        &source.premultiplied_bgra,
                        source.width,
                        source.height,
                        physical_rect_outward(sample_rect),
                        width,
                        height,
                    )?;
                    trace_duration("blur.crop", crop_start.elapsed());
                    Some(blurred)
                })??;
            let bytes = blurred.len();
            if !cache.can_store(bytes) {
                let result = draw(&blurred, width, height, style.opacity.clamp(0.0, 1.0));
                trace_duration("blur.total", total_start.elapsed());
                return Some(result);
            }
            cache.insert(key.clone(), blurred, bytes);
        } else {
            trace_duration("blur.cache_hit", total_start.elapsed());
        }
        let pixels = cache.get(&key)?;
        let result = draw(pixels, width, height, style.opacity.clamp(0.0, 1.0));
        trace_duration("blur.total", total_start.elapsed());
        Some(result)
    })
}

fn with_blurred_source<T>(
    source: &'static str,
    source_rect: UiRect,
    fit: lgui::core::ImageFit,
    style: BackdropBlurStyle,
    read: impl FnOnce(&image::RasterImage) -> T,
) -> Option<T> {
    let key = BlurredSourceKey {
        source,
        fit,
        source_rect,
        radius: style.radius.ceil().max(0.0) as usize,
        tint: style.tint.0,
        tint_alpha_bits: style.tint_alpha.to_bits(),
    };
    BLURRED_SOURCE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_touch(&key) {
            let source =
                with_source_raster(source, source_rect, fit, |source| image::RasterImage {
                    width: source.width,
                    height: source.height,
                    premultiplied_bgra: source.premultiplied_bgra.clone(),
                })?;
            let mut blurred = source.premultiplied_bgra;
            let blur_start = Instant::now();
            box_blur_bgra(
                &mut blurred,
                source.width as usize,
                source.height as usize,
                style.radius.ceil().max(0.0) as usize,
            );
            trace_duration("blur.source_box_blur", blur_start.elapsed());
            let tint_start = Instant::now();
            for pixel in blurred.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            apply_tint(&mut blurred, style.tint, style.tint_alpha);
            trace_duration("blur.source_tint", tint_start.elapsed());
            let raster = image::RasterImage {
                width: source.width,
                height: source.height,
                premultiplied_bgra: blurred,
            };
            let bytes = raster.premultiplied_bgra.len();
            if !cache.can_store(bytes) {
                return Some(read(&raster));
            }
            cache.insert(key.clone(), raster, bytes);
        }
        cache.get(&key).map(read)
    })
}

fn with_source_raster<T>(
    source: &'static str,
    source_rect: UiRect,
    fit: lgui::core::ImageFit,
    read: impl FnOnce(&image::RasterImage) -> T,
) -> Option<T> {
    let key = SourceRasterKey {
        source,
        fit,
        source_rect,
    };
    SOURCE_RASTER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_touch(&key) {
            let raster_start = Instant::now();
            let raster = image::rasterize_image_bgra(source, source_rect, fit)?;
            trace_duration("blur.rasterize_source", raster_start.elapsed());
            let bytes = raster.premultiplied_bgra.len();
            if !cache.can_store(bytes) {
                return Some(read(&raster));
            }
            cache.insert(key.clone(), raster, bytes);
        }
        cache.get(&key).map(read)
    })
}

fn physical_rect_outward(rect: UiRect) -> PhysicalRect {
    PhysicalRect::new(
        rect.left.floor() as i32,
        rect.top.floor() as i32,
        rect.right.ceil() as i32,
        rect.bottom.ceil() as i32,
    )
}

fn crop_bgra(
    source: &[u8],
    source_width: i32,
    source_height: i32,
    rect: PhysicalRect,
    width: i32,
    height: i32,
) -> Option<Vec<u8>> {
    if source_width <= 0
        || source_height <= 0
        || width <= 0
        || height <= 0
        || source.len() != (source_width * source_height * 4) as usize
    {
        return None;
    }

    let mut out = vec![0u8; (width * height * 4) as usize];
    if rect.left >= 0 && rect.top >= 0 && rect.right <= source_width && rect.bottom <= source_height
    {
        let row_bytes = (width * 4) as usize;
        for y in 0..height {
            let source_index = (((rect.top + y) * source_width + rect.left) * 4) as usize;
            let target_index = (y * width * 4) as usize;
            out[target_index..target_index + row_bytes]
                .copy_from_slice(&source[source_index..source_index + row_bytes]);
        }
        return Some(out);
    }

    for y in 0..height {
        let source_y = (rect.top + y).clamp(0, source_height - 1);
        let target_row_start = (y * width * 4) as usize;
        let target_row = &mut out[target_row_start..target_row_start + (width * 4) as usize];
        let copy_left = rect.left.clamp(0, source_width);
        let copy_right = rect.right.clamp(0, source_width);

        if copy_left < copy_right {
            let target_x = copy_left - rect.left;
            if target_x > 0 {
                let edge_index = ((source_y * source_width + copy_left) * 4) as usize;
                fill_bgra(
                    &mut target_row[..(target_x * 4) as usize],
                    &source[edge_index..edge_index + 4],
                );
            }

            let copy_width = copy_right - copy_left;
            let source_index = ((source_y * source_width + copy_left) * 4) as usize;
            let target_index = (target_x * 4) as usize;
            let copy_bytes = (copy_width * 4) as usize;
            target_row[target_index..target_index + copy_bytes]
                .copy_from_slice(&source[source_index..source_index + copy_bytes]);

            let right_start = target_index + copy_bytes;
            if right_start < target_row.len() {
                let edge_index = ((source_y * source_width + copy_right - 1) * 4) as usize;
                fill_bgra(
                    &mut target_row[right_start..],
                    &source[edge_index..edge_index + 4],
                );
            }
        } else {
            let source_x = rect.left.clamp(0, source_width - 1);
            let edge_index = ((source_y * source_width + source_x) * 4) as usize;
            fill_bgra(target_row, &source[edge_index..edge_index + 4]);
        }
    }
    Some(out)
}

fn fill_bgra(target: &mut [u8], pixel: &[u8]) {
    for chunk in target.chunks_exact_mut(4) {
        chunk.copy_from_slice(pixel);
    }
}

fn box_blur_bgra(pixels: &mut [u8], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }

    let mut temp = vec![0u8; pixels.len()];
    blur_horizontal_bgra(pixels, &mut temp, width, height, radius);
    blur_vertical_bgra(&temp, pixels, width, height, radius);
}

fn blur_horizontal_bgra(
    source: &[u8],
    target: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
) {
    for y in 0..height {
        let row_start = y * width * 4;
        let mut sum = [0u32; 4];
        let initial_right = radius.min(width - 1);
        for sample_x in 0..=initial_right {
            let index = row_start + sample_x * 4;
            sum[0] += source[index] as u32;
            sum[1] += source[index + 1] as u32;
            sum[2] += source[index + 2] as u32;
            sum[3] += source[index + 3] as u32;
        }

        for x in 0..width {
            let left = x.saturating_sub(radius);
            let right = (x + radius).min(width - 1);
            let count = (right - left + 1) as u32;
            let index = row_start + x * 4;
            target[index] = (sum[0] / count) as u8;
            target[index + 1] = (sum[1] / count) as u8;
            target[index + 2] = (sum[2] / count) as u8;
            target[index + 3] = (sum[3] / count) as u8;

            let next_x = x + 1;
            if next_x < width {
                if next_x > radius {
                    let remove_x = next_x - radius - 1;
                    let remove_index = row_start + remove_x * 4;
                    sum[0] -= source[remove_index] as u32;
                    sum[1] -= source[remove_index + 1] as u32;
                    sum[2] -= source[remove_index + 2] as u32;
                    sum[3] -= source[remove_index + 3] as u32;
                }

                let add_x = next_x + radius;
                if add_x < width {
                    let add_index = row_start + add_x * 4;
                    sum[0] += source[add_index] as u32;
                    sum[1] += source[add_index + 1] as u32;
                    sum[2] += source[add_index + 2] as u32;
                    sum[3] += source[add_index + 3] as u32;
                }
            }
        }
    }
}

fn blur_vertical_bgra(
    source: &[u8],
    target: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
) {
    for x in 0..width {
        let mut sum = [0u32; 4];
        let initial_bottom = radius.min(height - 1);
        for sample_y in 0..=initial_bottom {
            let index = (sample_y * width + x) * 4;
            sum[0] += source[index] as u32;
            sum[1] += source[index + 1] as u32;
            sum[2] += source[index + 2] as u32;
            sum[3] += source[index + 3] as u32;
        }

        for y in 0..height {
            let top = y.saturating_sub(radius);
            let bottom = (y + radius).min(height - 1);
            let count = (bottom - top + 1) as u32;
            let index = (y * width + x) * 4;
            target[index] = (sum[0] / count) as u8;
            target[index + 1] = (sum[1] / count) as u8;
            target[index + 2] = (sum[2] / count) as u8;
            target[index + 3] = (sum[3] / count) as u8;

            let next_y = y + 1;
            if next_y < height {
                if next_y > radius {
                    let remove_y = next_y - radius - 1;
                    let remove_index = (remove_y * width + x) * 4;
                    sum[0] -= source[remove_index] as u32;
                    sum[1] -= source[remove_index + 1] as u32;
                    sum[2] -= source[remove_index + 2] as u32;
                    sum[3] -= source[remove_index + 3] as u32;
                }

                let add_y = next_y + radius;
                if add_y < height {
                    let add_index = (add_y * width + x) * 4;
                    sum[0] += source[add_index] as u32;
                    sum[1] += source[add_index + 1] as u32;
                    sum[2] += source[add_index + 2] as u32;
                    sum[3] += source[add_index + 3] as u32;
                }
            }
        }
    }
}

fn apply_tint(pixels: &mut [u8], color: lgui::core::Color, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let inv = 1.0 - alpha;
    let red = ((color.0 >> 16) & 0xFF) as f32;
    let green = ((color.0 >> 8) & 0xFF) as f32;
    let blue = (color.0 & 0xFF) as f32;
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[0] = (blue * alpha + pixel[0] as f32 * inv).round() as u8;
        pixel[1] = (green * alpha + pixel[1] as f32 * inv).round() as u8;
        pixel[2] = (red * alpha + pixel[2] as f32 * inv).round() as u8;
    }
}

fn hash_rect<H: Hasher>(rect: &UiRect, state: &mut H) {
    rect.left.to_bits().hash(state);
    rect.top.to_bits().hash(state);
    rect.right.to_bits().hash(state);
    rect.bottom.to_bits().hash(state);
}

fn trace_duration(label: &str, duration: std::time::Duration) {
    if render_trace::duration_enabled(label) {
        let elapsed_ms = duration.as_secs_f64() * 1000.0;
        let threshold_ms = if render_trace::duration_enabled("blur-detail") {
            0.0
        } else if label == "blur.total" {
            2.0
        } else {
            0.5
        };
        if elapsed_ms >= threshold_ms {
            eprintln!("[ui-trace] {label}: {elapsed_ms:.2}ms");
        }
    }
}

fn trace_blur_miss(rect: UiRect, width: i32, height: i32, radius: usize) {
    if render_trace::duration_enabled("blur-detail")
        || (render_trace::duration_enabled("blur") && width * height >= 4096)
    {
        eprintln!(
            "[ui-trace] blur.cache_miss: rect={rect:?} size={}x{} radius={}",
            width, height, radius
        );
    }
}
