use windows::Win32::Graphics::Gdi::HDC;

use super::GdiRenderer;
use lgui::core::{Scene, UiRect};
use lgui::renderer::RenderBackend;

#[derive(Default)]
pub struct GdiRenderBackend;

impl RenderBackend<HDC> for GdiRenderBackend {
    fn draw_scene(&mut self, target: HDC, list: &Scene, clip: Option<UiRect>) {
        GdiRenderer::draw_scene_clipped(target, list, clip);
    }
}
