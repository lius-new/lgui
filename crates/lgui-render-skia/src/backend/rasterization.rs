//! The same raster policy is used by native paragraphs and screen surfaces.
//! LCD coverage is meaningful only on a known, opaque, unscaled target.
use skia_safe::{
    AlphaType, Canvas, ColorType, FontHinting, PixelGeometry, SurfaceProps, SurfacePropsFlags,
    font::Edging, textlayout::TextStyle,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextRasterization {
    #[default]
    Grayscale,
    LcdRgb,
    LcdBgr,
}

impl TextRasterization {
    /// Uses the system's smoothing preference, never assumes an RGB panel when
    /// its order is unknown. Rechecked on rendering rather than frozen at startup.
    pub fn system() -> Self {
        use std::{
            cell::RefCell,
            time::{Duration, Instant},
        };
        thread_local! {
            static SYSTEM: RefCell<Option<(Instant, TextRasterization)>> = const { RefCell::new(None) };
        }
        SYSTEM.with(|cached| {
            let mut cached = cached.borrow_mut();
            if let Some((checked, mode)) = *cached
                && checked.elapsed() < Duration::from_millis(500)
            {
                return mode;
            }
            let mode = system_rasterization();
            *cached = Some((Instant::now(), mode));
            mode
        })
    }

    pub fn configure(self, style: &mut TextStyle) {
        style
            .set_font_edging(match self {
                Self::Grayscale => Edging::AntiAlias,
                Self::LcdRgb | Self::LcdBgr => Edging::SubpixelAntiAlias,
            })
            // Subpixel positioning and LCD antialiasing are separate controls.
            .set_subpixel(true)
            .set_font_hinting(FontHinting::Slight);
    }

    pub fn surface_props(self) -> SurfaceProps {
        SurfaceProps::new(
            SurfacePropsFlags::empty(),
            match self {
                Self::Grayscale => PixelGeometry::Unknown,
                Self::LcdRgb => PixelGeometry::RGBH,
                Self::LcdBgr => PixelGeometry::BGRH,
            },
        )
    }

    /// Apply explicit LCD coverage overrides. Parameter selection belongs to
    /// the caller; grayscale fallback targets retain backend defaults.
    pub fn surface_props_with_options(
        self,
        options: lgui_core::text::TextRenderingOptions,
    ) -> SurfaceProps {
        let defaults = self.surface_props();
        if self != Self::Grayscale {
            SurfaceProps::new_with_text_properties(
                SurfacePropsFlags::empty(),
                defaults.pixel_geometry(),
                options.lcd_contrast().unwrap_or(defaults.text_contrast()),
                options.lcd_gamma().unwrap_or(defaults.text_gamma()),
            )
        } else {
            defaults
        }
    }

    pub(super) fn for_canvas(canvas: &Canvas, alpha: u8) -> Self {
        let info = canvas.image_info();
        if alpha != 255
            || info.alpha_type() != AlphaType::Opaque
            || info.color_type() != ColorType::N32
            || !canvas.local_to_device_as_3x3().is_translate()
        {
            return Self::Grayscale;
        }
        // Use the out-parameter binding. Skia 0.99's getTopProps() returns a
        // C++ struct by value and faults through its Windows/MSVC Rust binding.
        // Our offscreen layers own separate canvases and were excluded above.
        match canvas.props().map(|props| props.pixel_geometry()) {
            Some(PixelGeometry::RGBH) => Self::LcdRgb,
            Some(PixelGeometry::BGRH) => Self::LcdBgr,
            _ => Self::Grayscale,
        }
    }
}

#[cfg(target_os = "windows")]
fn system_rasterization() -> TextRasterization {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FE_FONTSMOOTHINGCLEARTYPE, FE_FONTSMOOTHINGORIENTATIONBGR, FE_FONTSMOOTHINGORIENTATIONRGB,
        SPI_GETFONTSMOOTHING, SPI_GETFONTSMOOTHINGORIENTATION, SPI_GETFONTSMOOTHINGTYPE,
        SystemParametersInfoW,
    };
    let (mut enabled, mut kind, mut orientation) = (0i32, 0u32, u32::MAX);
    // Each GET writes exactly the documented scalar type. No settings are changed.
    unsafe {
        if SystemParametersInfoW(
            SPI_GETFONTSMOOTHING,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        ) == 0
            || enabled == 0
            || SystemParametersInfoW(
                SPI_GETFONTSMOOTHINGTYPE,
                0,
                (&mut kind as *mut u32).cast(),
                0,
            ) == 0
            || kind != FE_FONTSMOOTHINGCLEARTYPE
            || SystemParametersInfoW(
                SPI_GETFONTSMOOTHINGORIENTATION,
                0,
                (&mut orientation as *mut u32).cast(),
                0,
            ) == 0
        {
            return TextRasterization::Grayscale;
        }
    }
    match orientation {
        FE_FONTSMOOTHINGORIENTATIONRGB => TextRasterization::LcdRgb,
        FE_FONTSMOOTHINGORIENTATIONBGR => TextRasterization::LcdBgr,
        _ => TextRasterization::Grayscale,
    }
}

#[cfg(not(target_os = "windows"))]
fn system_rasterization() -> TextRasterization {
    TextRasterization::Grayscale
}
