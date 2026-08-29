#![allow(dead_code)]

pub mod backend;
pub mod blur;
pub mod custom_paint;
pub mod d2d;
pub mod gdi_renderer;
pub mod image;
pub mod static_layer;
pub mod static_layer_raster_cache;

pub use gdi_renderer::{
    clear_gdi_renderer_caches, release_gdi_compositing_layer_scope, reset_gdi_frame_blit_metrics,
    take_gdi_frame_blit_metrics, GdiFrameBlitMetrics, GdiFrameBlitSourceMetrics, GdiRenderer,
};

pub fn draw_scene(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    scene: &lgui::core::Scene,
    clip: Option<lgui::core::UiRect>,
) {
    use lgui::renderer::RenderBackend as _;
    backend::GdiRenderBackend.draw_scene(hdc, scene, clip);
}
