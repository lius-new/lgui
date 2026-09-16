use std::{cell::RefCell, sync::OnceLock, time::Duration, time::Instant};

use windows::Win32::{
    Graphics::{
        Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
        },
        GdiPlus::{
            GdipCreateFromHDC, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectRectI,
            GdipGetImageHeight, GdipGetImageWidth, GdipLoadImageFromStream,
            GdipSetInterpolationMode, GdipSetPixelOffsetMode, GdipSetSmoothingMode, GpGraphics,
            GpImage, InterpolationModeHighQualityBicubic, Ok as GpOk, PixelOffsetModeHalf,
            SmoothingModeHighQuality, UnitPixel,
        },
    },
    UI::Shell::SHCreateMemStream,
};

use crate::{self as cached_image, render_trace};
use lgui_core::core::{ImageFit, UiImageSource, UiRect};

fn decoded_image_telemetry() -> &'static lgui_core::memory::CacheTelemetry {
    static TELEMETRY: OnceLock<lgui_core::memory::CacheTelemetry> = OnceLock::new();
    TELEMETRY.get_or_init(Default::default)
}

thread_local! {
    static DECODED_IMAGE_CACHE: RefCell<lgui_core::memory::LruCache<String, DecodedImage>> = RefCell::new(
        lgui_core::memory::LruCache::new(
            0,
            lgui_core::memory::ResourceClass::Cache,
            decoded_image_telemetry().clone(),
        )
    );
}

fn raster_length(value: f32) -> i32 {
    value.ceil().max(1.0) as i32
}

struct DecodedImage {
    image: *mut GpImage,
    width: i32,
    height: i32,
}

impl Drop for DecodedImage {
    fn drop(&mut self) {
        if !self.image.is_null() {
            unsafe {
                let _ = GdipDisposeImage(self.image);
            }
        }
    }
}

pub(crate) fn decoded_image_cache_usage() -> lgui_core::memory::CacheUsage {
    decoded_image_telemetry().snapshot()
}

pub(crate) fn trim_decoded_image_cache(target_bytes: usize) -> usize {
    DECODED_IMAGE_CACHE.with(|cache| cache.borrow_mut().trim_to(target_bytes))
}

pub(crate) fn set_decoded_image_cache_budget(budget_bytes: usize) {
    DECODED_IMAGE_CACHE.with(|cache| cache.borrow_mut().set_budget(budget_bytes));
}

fn with_cached_decoded<T>(
    key: String,
    create: impl FnOnce() -> Option<DecodedImage>,
    read: impl FnOnce(&DecodedImage) -> T,
) -> Option<T> {
    DECODED_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_touch(&key) {
            let image = create()?;
            let bytes = decoded_image_bytes(&image)?;
            if !cache.can_store(bytes) {
                return Some(read(&image));
            }
            cache.insert(key.clone(), image, bytes);
        }
        cache.get(&key).map(read)
    })
}

fn decoded_image_bytes(image: &DecodedImage) -> Option<usize> {
    usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?
        .checked_mul(4)
}

pub fn draw_image(hdc: HDC, rect: UiRect, source: &'static str, fit: ImageFit) {
    let start = Instant::now();
    if is_remote_url(source) {
        draw_remote_image(hdc, rect, source, fit);
        trace_duration("gdi.draw_image", start.elapsed());
        return;
    }
    let _ = with_cached_decoded(
        source.to_string(),
        || decode_image(source),
        |image| draw_decoded_image(hdc, rect, image, fit),
    );
    trace_duration("gdi.draw_image", start.elapsed());
}

pub fn draw_ui_image(hdc: HDC, rect: UiRect, source: &UiImageSource, fit: ImageFit) {
    match source {
        UiImageSource::Static(source) => draw_image(hdc, rect, source, fit),
        UiImageSource::Url(url) => {
            draw_cached_source(hdc, rect, &cached_image::ImageSource::url(url), fit)
        }
        UiImageSource::File(path) => {
            draw_cached_source(hdc, rect, &cached_image::ImageSource::file(path), fit)
        }
        UiImageSource::Bytes {
            key,
            version,
            bytes,
        } => draw_bytes_image(hdc, rect, key, *version, bytes, fit),
    }
}

fn draw_bytes_image(hdc: HDC, rect: UiRect, key: &str, version: u64, bytes: &[u8], fit: ImageFit) {
    let cache_key = format!("bytes:{key}:{version}");
    let _ = with_cached_decoded(
        cache_key,
        || decode_image_bytes(bytes, 0, 0),
        |image| draw_decoded_image(hdc, rect, image, fit),
    );
}

fn draw_cached_source(hdc: HDC, rect: UiRect, source: &cached_image::ImageSource, fit: ImageFit) {
    let key = match source {
        cached_image::ImageSource::Url(url) => format!("url:{url}"),
        cached_image::ImageSource::File(path) => format!("file:{}", path.display()),
        cached_image::ImageSource::Asset { key, .. } => format!("asset:{key}"),
    };
    let _ = with_cached_decoded(
        key,
        || {
            let (bytes, width, height) = cached_image::cached_image_data(source)?;
            decode_image_bytes(&bytes, width, height)
        },
        |image| draw_decoded_image(hdc, rect, image, fit),
    );
}

fn draw_remote_image(hdc: HDC, rect: UiRect, source: &'static str, fit: ImageFit) {
    let _ = with_cached_decoded(
        source.to_string(),
        || {
            let cached_source = cached_image::ImageSource::url(source);
            let (bytes, width, height) = cached_image::cached_image_data(&cached_source)?;
            decode_image_bytes(&bytes, width, height)
        },
        |image| draw_decoded_image(hdc, rect, image, fit),
    );
}

fn is_remote_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

#[allow(dead_code)]
pub struct RasterImage {
    pub width: i32,
    pub height: i32,
    pub premultiplied_bgra: Vec<u8>,
}

pub fn rasterize_image_bgra(
    source: &'static str,
    rect: UiRect,
    fit: ImageFit,
) -> Option<RasterImage> {
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    if is_remote_url(source) {
        return rasterize_remote_image_bgra(source, width, height, fit);
    }
    with_cached_decoded(
        source.to_string(),
        || decode_image(source),
        |image| rasterize_decoded_image(image, width, height, fit),
    )?
}

pub fn rasterize_ui_image_bgra(
    source: &UiImageSource,
    rect: UiRect,
    fit: ImageFit,
) -> Option<RasterImage> {
    match source {
        UiImageSource::Static(source) => rasterize_image_bgra(source, rect, fit),
        UiImageSource::Url(url) => {
            rasterize_cached_image_bgra(&cached_image::ImageSource::url(url), rect, fit)
        }
        UiImageSource::File(path) => {
            rasterize_cached_image_bgra(&cached_image::ImageSource::file(path), rect, fit)
        }
        UiImageSource::Bytes { bytes, .. } => rasterize_image_bytes_bgra(bytes, rect, fit),
    }
}

fn rasterize_image_bytes_bgra(bytes: &[u8], rect: UiRect, fit: ImageFit) -> Option<RasterImage> {
    let image = decode_image_bytes(bytes, 0, 0)?;
    rasterize_decoded_image(
        &image,
        raster_length(rect.width()),
        raster_length(rect.height()),
        fit,
    )
}

fn rasterize_cached_image_bgra(
    source: &cached_image::ImageSource,
    rect: UiRect,
    fit: ImageFit,
) -> Option<RasterImage> {
    let width = raster_length(rect.width());
    let height = raster_length(rect.height());
    let key = match source {
        cached_image::ImageSource::Url(url) => format!("url:{url}"),
        cached_image::ImageSource::File(path) => format!("file:{}", path.display()),
        cached_image::ImageSource::Asset { key, .. } => format!("asset:{key}"),
    };
    with_cached_decoded(
        key,
        || {
            let (bytes, image_width, image_height) = cached_image::cached_image_data(source)?;
            decode_image_bytes(&bytes, image_width, image_height)
        },
        |image| rasterize_decoded_image(image, width, height, fit),
    )?
}

fn rasterize_remote_image_bgra(
    source: &'static str,
    width: i32,
    height: i32,
    fit: ImageFit,
) -> Option<RasterImage> {
    with_cached_decoded(
        source.to_string(),
        || {
            let cached_source = cached_image::ImageSource::url(source);
            let (bytes, image_width, image_height) =
                cached_image::cached_image_data(&cached_source)?;
            decode_image_bytes(&bytes, image_width, image_height)
        },
        |image| rasterize_decoded_image(image, width, height, fit),
    )?
}

fn decode_image(source: &'static str) -> Option<DecodedImage> {
    let bytes = lgui_core::backend::render_resources()
        .resolver()?
        .resolve(source)
        .ok()?;
    decode_image_from_bytes(&bytes)
}

fn decode_image_bytes(
    bytes: &[u8],
    expected_width: i32,
    expected_height: i32,
) -> Option<DecodedImage> {
    let image = decode_image_from_bytes(bytes)?;
    if expected_width > 0
        && expected_height > 0
        && (image.width != expected_width || image.height != expected_height)
    {
        return None;
    }
    Some(image)
}

fn decode_image_from_bytes(bytes: &[u8]) -> Option<DecodedImage> {
    unsafe {
        let stream = SHCreateMemStream(Some(bytes))?;
        let mut image: *mut GpImage = std::ptr::null_mut();
        if GdipLoadImageFromStream(&stream, &mut image) != GpOk || image.is_null() {
            return None;
        }

        let mut width = 0u32;
        let mut height = 0u32;
        if GdipGetImageWidth(image, &mut width) != GpOk
            || GdipGetImageHeight(image, &mut height) != GpOk
            || width == 0
            || height == 0
        {
            let _ = GdipDisposeImage(image);
            return None;
        }

        Some(DecodedImage {
            image,
            width: i32::try_from(width).ok()?,
            height: i32::try_from(height).ok()?,
        })
    }
}

fn draw_decoded_image(hdc: HDC, rect: UiRect, image: &DecodedImage, fit: ImageFit) {
    if image.image.is_null() || image.width <= 0 || image.height <= 0 {
        return;
    }
    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return;
        }

        let dest_left = rect.left.round() as i32;
        let dest_top = rect.top.round() as i32;
        let dest_width = raster_length(rect.width());
        let dest_height = raster_length(rect.height());
        let ImageDrawLayout {
            dest_x,
            dest_y,
            dest_width,
            dest_height,
            source_x,
            source_y,
            crop_width,
            crop_height,
        } = image_draw_layout(
            image.width,
            image.height,
            dest_left,
            dest_top,
            dest_width,
            dest_height,
            fit,
        );

        let _ = GdipDrawImageRectRectI(
            graphics,
            image.image,
            dest_x,
            dest_y,
            dest_width,
            dest_height,
            source_x,
            source_y,
            crop_width,
            crop_height,
            UnitPixel,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
        );

        let _ = GdipDeleteGraphics(graphics);
    }
}

fn rasterize_decoded_image(
    image: &DecodedImage,
    width: i32,
    height: i32,
    fit: ImageFit,
) -> Option<RasterImage> {
    if image.image.is_null() || image.width <= 0 || image.height <= 0 || width <= 0 || height <= 0 {
        return None;
    }

    unsafe {
        let memory_dc = CreateCompatibleDC(None);
        if memory_dc.is_invalid() {
            return None;
        }
        let mut bits = std::ptr::null_mut();
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let Ok(bitmap) = CreateDIBSection(None, &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(memory_dc);
            return None;
        };
        if bitmap.is_invalid() || bits.is_null() {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            return None;
        }
        std::ptr::write_bytes(bits, 0, (width * height * 4) as usize);
        let old_bitmap = SelectObject(memory_dc, bitmap.into());

        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(memory_dc, &mut graphics) != GpOk || graphics.is_null() {
            let _ = SelectObject(memory_dc, old_bitmap);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            return None;
        }

        let _ = GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        let _ = GdipSetSmoothingMode(graphics, SmoothingModeHighQuality);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);

        let ImageDrawLayout {
            dest_x,
            dest_y,
            dest_width,
            dest_height,
            source_x,
            source_y,
            crop_width,
            crop_height,
        } = image_draw_layout(image.width, image.height, 0, 0, width, height, fit);
        let _ = GdipDrawImageRectRectI(
            graphics,
            image.image,
            dest_x,
            dest_y,
            dest_width,
            dest_height,
            source_x,
            source_y,
            crop_width,
            crop_height,
            UnitPixel,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
        );

        let _ = GdipDeleteGraphics(graphics);
        let len = (width * height * 4) as usize;
        let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), len).to_vec();
        let _ = SelectObject(memory_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_dc);
        Some(RasterImage {
            width,
            height,
            premultiplied_bgra: pixels,
        })
    }
}

struct ImageDrawLayout {
    dest_x: i32,
    dest_y: i32,
    dest_width: i32,
    dest_height: i32,
    source_x: i32,
    source_y: i32,
    crop_width: i32,
    crop_height: i32,
}

fn image_draw_layout(
    source_width: i32,
    source_height: i32,
    dest_left: i32,
    dest_top: i32,
    dest_width: i32,
    dest_height: i32,
    fit: ImageFit,
) -> ImageDrawLayout {
    let (source_x, source_y, crop_width, crop_height) =
        image_crop(source_width, source_height, dest_width, dest_height, fit);
    let (draw_width, draw_height) =
        image_fit_size(source_width, source_height, dest_width, dest_height, fit);
    let draw_x = dest_left + (dest_width - draw_width) / 2;
    let draw_y = dest_top + (dest_height - draw_height) / 2;

    ImageDrawLayout {
        dest_x: draw_x,
        dest_y: draw_y,
        dest_width: draw_width,
        dest_height: draw_height,
        source_x,
        source_y,
        crop_width,
        crop_height,
    }
}

fn image_fit_size(
    source_width: i32,
    source_height: i32,
    dest_width: i32,
    dest_height: i32,
    fit: ImageFit,
) -> (i32, i32) {
    match fit {
        ImageFit::Fill | ImageFit::Cover => (dest_width.max(1), dest_height.max(1)),
        ImageFit::Contain => {
            let scale = f32::min(
                dest_width as f32 / source_width.max(1) as f32,
                dest_height as f32 / source_height.max(1) as f32,
            );
            let width = ((source_width as f32 * scale).round() as i32).clamp(1, dest_width.max(1));
            let height =
                ((source_height as f32 * scale).round() as i32).clamp(1, dest_height.max(1));
            (width, height)
        }
    }
}

fn image_crop(
    source_width: i32,
    source_height: i32,
    dest_width: i32,
    dest_height: i32,
    fit: ImageFit,
) -> (i32, i32, i32, i32) {
    match fit {
        ImageFit::Fill => (0, 0, source_width, source_height),
        ImageFit::Contain => (0, 0, source_width, source_height),
        ImageFit::Cover => {
            let scale = f32::max(
                dest_width as f32 / source_width as f32,
                dest_height as f32 / source_height as f32,
            );
            let crop_width = ((dest_width as f32 / scale).round() as i32).clamp(1, source_width);
            let crop_height = ((dest_height as f32 / scale).round() as i32).clamp(1, source_height);
            let source_x = ((source_width - crop_width) / 2).max(0);
            let source_y = ((source_height - crop_height) / 2).max(0);
            (source_x, source_y, crop_width, crop_height)
        }
    }
}

fn trace_duration(label: &str, duration: Duration) {
    if render_trace::duration_enabled(label) {
        eprintln!(
            "[ui-trace] {label}: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        );
    }
}
