use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{GetDC, ReleaseDC},
};

use lgui_render_api::{FrameInfo, RenderStats, RendererCapabilities, SceneRenderer};

use crate::{
    GdiRenderer, Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct GdiRendererFactory;

impl Win32RendererFactory for GdiRendererFactory {
    fn name(&self) -> &'static str {
        "gdi"
    }

    fn create(&self, _hwnd: HWND) -> windows::core::Result<Box<Win32SceneRenderer>> {
        Ok(Box::new(GdiRenderer::default()))
    }

    #[cfg(feature = "advanced-rendering")]
    fn install_environment(
        &self,
        context: &lgui_core::application::ApplicationContext,
        dispatcher: &lgui_platform_win32::Win32Dispatcher,
    ) -> Box<dyn std::any::Any> {
        crate::environment::install_gdi(context, dispatcher)
    }

    #[cfg(all(feature = "diagnostics", feature = "advanced-rendering"))]
    fn reset_present_metrics(&self) {
        crate::enhanced::reset_gdi_frame_blit_metrics();
    }

    #[cfg(all(feature = "diagnostics", feature = "advanced-rendering"))]
    fn take_present_metrics(
        &self,
        submitted_pixels: u64,
    ) -> lgui_core::diagnostics::FramePresentMetrics {
        use lgui_core::diagnostics::{FrameBlitSourceMetrics, FramePresentMetrics};

        fn source(metrics: crate::enhanced::GdiFrameBlitSourceMetrics) -> FrameBlitSourceMetrics {
            FrameBlitSourceMetrics {
                blit_count: metrics.blit_count,
                bitblt_count: metrics.bitblt_count,
                alphablend_count: metrics.alphablend_count,
                fallback_count: metrics.fallback_count,
                pixels: metrics.pixels,
            }
        }

        let metrics = crate::enhanced::take_gdi_frame_blit_metrics();
        FramePresentMetrics {
            submitted_pixels,
            blit_count: metrics.blit_count,
            bitblt_count: metrics.bitblt_count,
            alphablend_count: metrics.alphablend_count,
            fallback_count: metrics.fallback_count,
            blit_pixels: metrics.blit_pixels,
            static_layer_blits: source(metrics.static_layer),
            overlay_blits: source(metrics.overlay),
            backdrop_blits: source(metrics.backdrop),
            custom_blits: source(metrics.custom),
            other_blits: source(metrics.other),
            ..FramePresentMetrics::default()
        }
    }
}

impl SceneRenderer for GdiRenderer {
    type Target = Win32RenderTarget;
    type Error = Win32RenderError;

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            partial_redraw: true,
            retained_surface: true,
        }
    }

    fn render(
        &mut self,
        target: &mut Self::Target,
        scene: &lgui_core::core::Scene,
        frame: &FrameInfo<'_>,
    ) -> Result<RenderStats, Self::Error> {
        let damage = frame.damage();
        if damage.is_empty() {
            self.draw_retained(target.hdc(), scene, frame.viewport(), damage)?;
            return Ok(RenderStats::for_frame(frame));
        }
        let window_target = unsafe { GetDC(Some(target.hwnd())) };
        if window_target.is_invalid() {
            self.draw_retained(target.hdc(), scene, frame.viewport(), damage)?;
            return Ok(RenderStats::for_frame(frame));
        }
        let presented = self.draw_retained(window_target, scene, frame.viewport(), damage);
        unsafe {
            let _ = ReleaseDC(Some(target.hwnd()), window_target);
        }
        presented?;
        Ok(RenderStats::for_frame(frame))
    }

    fn memory_usage(&self) -> lgui_core::memory::CacheUsage {
        GdiRenderer::memory_usage(self)
    }
}
