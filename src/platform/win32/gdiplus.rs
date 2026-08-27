use windows::{
    core::{Error, Result, HRESULT},
    Win32::Graphics::GdiPlus::{GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, Ok as GpOk},
};

/// Keeps GDI+ alive for every image decoder owned by a Win32 application.
pub(super) struct GdiPlusRuntime {
    token: usize,
}

impl GdiPlusRuntime {
    pub(super) fn start() -> Result<Self> {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        let mut token = 0;
        let status = unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) };
        if status != GpOk || token == 0 {
            return Err(Error::new(
                HRESULT(0x8000_4005_u32 as i32),
                format!("GDI+ startup failed with status {}", status.0),
            ));
        }
        Ok(Self { token })
    }
}

impl Drop for GdiPlusRuntime {
    fn drop(&mut self) {
        // Every cached GpImage must be disposed before the process-wide GDI+ token is released.
        super::clear_cached_decoded_image_cache();
        #[cfg(any(feature = "advanced-rendering", feature = "renderer-d2d"))]
        super::enhanced::image::clear_decoded_image_cache();
        unsafe {
            GdiplusShutdown(self.token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GdiPlusRuntime;

    #[test]
    fn startup_produces_a_live_gdiplus_token() {
        let runtime = GdiPlusRuntime::start().expect("GDI+ should start on Windows");
        assert_ne!(runtime.token, 0);
    }

    #[cfg(any(feature = "advanced-rendering", feature = "renderer-d2d"))]
    #[test]
    fn runtime_enables_png_decoding_for_the_image_renderer() {
        use crate::core::{ImageFit, UiImageSource, UiRect};

        const ONE_PIXEL_PNG: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00,
            0x00, 0xB5, 0x1C, 0x0C, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78,
            0xDA, 0x63, 0x64, 0xF8, 0x0F, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xE3, 0x66,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let _runtime = GdiPlusRuntime::start().expect("GDI+ should start on Windows");
        let image = super::super::enhanced::image::rasterize_ui_image_bgra(
            &UiImageSource::bytes("test.pixel", 1, ONE_PIXEL_PNG.to_vec()),
            UiRect::new(0, 0, 1, 1),
            ImageFit::Fill,
        )
        .expect("PNG should decode while the runtime is alive");

        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.premultiplied_bgra.len(), 4);
    }
}
