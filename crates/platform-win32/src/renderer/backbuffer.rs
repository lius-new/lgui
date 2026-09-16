#[cfg(any(
    feature = "multi-window",
    all(feature = "renderer-gdi", not(feature = "advanced-rendering"))
))]
use std::ffi::c_void;

#[cfg(feature = "multi-window")]
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};

#[cfg(feature = "multi-window")]
use lgui_core::core::PhysicalRect;

pub struct LayeredBackbuffer {
    hdc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    #[cfg(any(
        feature = "multi-window",
        all(feature = "renderer-gdi", not(feature = "advanced-rendering"))
    ))]
    bits: *mut c_void,
    width: i32,
    height: i32,
    #[cfg(feature = "multi-window")]
    generation: u64,
    valid: bool,
    #[cfg(feature = "multi-window")]
    alpha_policy: AlphaPolicy,
}

#[cfg(feature = "multi-window")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaPolicy {
    Opaque,
}

impl LayeredBackbuffer {
    pub fn new(screen_dc: HDC, width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        unsafe {
            let hdc = CreateCompatibleDC(Some(screen_dc));
            if hdc.is_invalid() {
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
            let Ok(bitmap) = CreateDIBSection(
                Some(screen_dc),
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            ) else {
                let _ = DeleteDC(hdc);
                return None;
            };
            if bitmap.is_invalid() || bits.is_null() {
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(hdc);
                return None;
            }

            let old_bitmap = SelectObject(hdc, bitmap.into());
            Some(Self {
                hdc,
                bitmap,
                old_bitmap,
                #[cfg(any(
                    feature = "multi-window",
                    all(feature = "renderer-gdi", not(feature = "advanced-rendering"))
                ))]
                bits,
                width,
                height,
                #[cfg(feature = "multi-window")]
                generation: 0,
                valid: false,
                #[cfg(feature = "multi-window")]
                alpha_policy: AlphaPolicy::Opaque,
            })
        }
    }

    pub fn hdc(&self) -> HDC {
        self.hdc
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn byte_len(&self) -> usize {
        (self.width.max(0) as usize)
            .saturating_mul(self.height.max(0) as usize)
            .saturating_mul(4)
    }

    #[cfg(feature = "multi-window")]
    pub fn viewport(&self) -> PhysicalRect {
        PhysicalRect::new(0, 0, self.width, self.height)
    }

    #[cfg(any(
        feature = "multi-window",
        all(feature = "renderer-gdi", not(feature = "advanced-rendering"))
    ))]
    pub fn pixels(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.bits.cast::<u8>(),
                (self.width * self.height * 4) as usize,
            )
        }
    }

    #[cfg(any(
        feature = "multi-window",
        all(feature = "renderer-gdi", not(feature = "advanced-rendering"))
    ))]
    pub fn copy_pixels_from(&mut self, pixels: &[u8]) -> bool {
        let expected_len = (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(4);
        if pixels.len() != expected_len {
            return false;
        }
        unsafe {
            let target = std::slice::from_raw_parts_mut(self.bits.cast::<u8>(), expected_len);
            target.copy_from_slice(pixels);
        }
        true
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    #[cfg(feature = "multi-window")]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[cfg(feature = "multi-window")]
    pub fn alpha_policy(&self) -> AlphaPolicy {
        self.alpha_policy
    }

    pub fn mark_valid(&mut self) {
        self.valid = true;
    }

    #[cfg(feature = "multi-window")]
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.generation = self.generation.wrapping_add(1);
    }

    #[cfg(feature = "multi-window")]
    pub fn set_opaque_alpha(&mut self) {
        set_opaque_alpha(self.bits.cast(), self.width, self.height);
    }

    #[cfg(feature = "multi-window")]
    pub fn set_opaque_alpha_region(&mut self, rect: PhysicalRect) {
        let Some(rect) = rect.intersect(self.viewport()) else {
            return;
        };
        set_opaque_alpha_region(self.bits.cast(), self.width, self.height, rect);
    }

    #[cfg(feature = "multi-window")]
    pub fn set_rounded_rect_alpha_mask(&mut self, rect: PhysicalRect, radius: i32) {
        let Some(rect) = rect.intersect(self.viewport()) else {
            return;
        };
        set_rounded_rect_alpha_mask(self.bits.cast(), self.width, self.height, rect, radius);
    }

    #[cfg(feature = "multi-window")]
    pub fn clear_region(&mut self, rect: PhysicalRect) {
        let Some(rect) = rect.intersect(self.viewport()) else {
            return;
        };
        unsafe {
            let slice = std::slice::from_raw_parts_mut(
                self.bits.cast::<u8>(),
                (self.width * self.height * 4) as usize,
            );
            for y in rect.top..rect.bottom {
                let start = ((y * self.width + rect.left) * 4) as usize;
                let end = ((y * self.width + rect.right) * 4) as usize;
                slice[start..end].fill(0);
            }
        }
    }
}

impl Drop for LayeredBackbuffer {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.old_bitmap);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.hdc);
        }
    }
}

#[cfg(feature = "multi-window")]
fn set_opaque_alpha(bits: *mut u8, width: i32, height: i32) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }
    let len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits, len) };
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

#[cfg(feature = "multi-window")]
fn set_opaque_alpha_region(bits: *mut u8, width: i32, height: i32, rect: PhysicalRect) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }
    let len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits, len) };
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let index = ((y * width + x) * 4 + 3) as usize;
            pixels[index] = 255;
        }
    }
}

#[cfg(feature = "multi-window")]
fn set_rounded_rect_alpha_mask(
    bits: *mut u8,
    width: i32,
    height: i32,
    rect: PhysicalRect,
    radius: i32,
) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }
    let radius = radius.clamp(0, (rect.width().min(rect.height()) / 2).max(0)) as f32;
    let len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits, len) };
    for y in 0..height {
        for x in 0..width {
            let coverage = rounded_rect_coverage(x, y, rect, radius);
            let alpha = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
            let index = ((y * width + x) * 4) as usize;
            if alpha == 0 {
                pixels[index..index + 4].fill(0);
                continue;
            }
            if alpha < 255 {
                let alpha_u16 = alpha as u16;
                pixels[index] = ((pixels[index] as u16 * alpha_u16 + 127) / 255) as u8;
                pixels[index + 1] = ((pixels[index + 1] as u16 * alpha_u16 + 127) / 255) as u8;
                pixels[index + 2] = ((pixels[index + 2] as u16 * alpha_u16 + 127) / 255) as u8;
            }
            pixels[index + 3] = alpha;
        }
    }
}

#[cfg(feature = "multi-window")]
fn rounded_rect_coverage(x: i32, y: i32, rect: PhysicalRect, radius: f32) -> f32 {
    if radius <= 0.0 {
        return if x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom {
            1.0
        } else {
            0.0
        };
    }

    const SAMPLE_OFFSETS: [f32; 4] = [0.125, 0.375, 0.625, 0.875];
    let mut covered = 0;
    for sy in SAMPLE_OFFSETS {
        for sx in SAMPLE_OFFSETS {
            if rounded_rect_contains(x as f32 + sx, y as f32 + sy, rect, radius) {
                covered += 1;
            }
        }
    }
    covered as f32 / 16.0
}

#[cfg(feature = "multi-window")]
fn rounded_rect_contains(x: f32, y: f32, rect: PhysicalRect, radius: f32) -> bool {
    let left = rect.left as f32;
    let top = rect.top as f32;
    let right = rect.right as f32;
    let bottom = rect.bottom as f32;
    if x < left || x >= right || y < top || y >= bottom {
        return false;
    }
    let inner_left = left + radius;
    let inner_right = right - radius;
    let inner_top = top + radius;
    let inner_bottom = bottom - radius;
    if (x >= inner_left && x < inner_right) || (y >= inner_top && y < inner_bottom) {
        return true;
    }
    let cx = if x < inner_left {
        inner_left
    } else {
        inner_right
    };
    let cy = if y < inner_top {
        inner_top
    } else {
        inner_bottom
    };
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}

#[cfg(feature = "multi-window")]
pub fn rect_size(rect: RECT) -> (i32, i32) {
    (rect.right - rect.left, rect.bottom - rect.top)
}
