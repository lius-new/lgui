use windows::{
    core::{Error, Result, HRESULT},
    Win32::Graphics::GdiPlus::{GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, Ok as GpOk},
};

/// Keeps GDI+ alive for every image decoder owned by a Win32 application.
pub struct GdiPlusRuntime {
    token: usize,
}

impl GdiPlusRuntime {
    pub fn start() -> Result<Self> {
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
        super::trim_decoded_image_cache(0);
        unsafe {
            GdiplusShutdown(self.token);
        }
    }
}

#[cfg(test)]
#[path = "gdiplus_test.rs"]
mod tests;
