use super::*;

#[test]
fn basic_gdi_renders_and_clips_cached_shadow() {
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, GdiFlush};
    let dc = unsafe { CreateCompatibleDC(None) };
    let output = LayeredBackbuffer::new(dc, 64, 64).unwrap();
    let renderer = GdiRenderer::new(Color::WHITE);
    let scene = lgui_render_api::test_support::shadow_scene(255, 0.0);
    let bounds = UiRect::new(0.0, 0.0, 64.0, 64.0);
    renderer.clear(output.hdc(), bounds);
    renderer.draw_commands(output.hdc(), scene.commands());
    unsafe {
        let _ = GdiFlush();
    }
    let offset = (20 * 64 + 44) * 4;
    assert_eq!(&output.pixels()[offset..offset + 3], &[127, 127, 255]);
    let snapshot = output.pixels().to_vec();
    renderer.clear(output.hdc(), bounds);
    renderer.draw_commands(output.hdc(), scene.commands());
    unsafe {
        let _ = GdiFlush();
    }
    assert_eq!(snapshot, output.pixels());
    renderer.clear(output.hdc(), bounds);
    renderer.draw_command(
        output.hdc(),
        &ScenePrimitive::Clip {
            id: lgui_core::core::UiId::new("clip"),
            rect: UiRect::new(0.0, 0.0, 32.0, 64.0),
            commands: scene.commands().to_vec(),
            child_signature: 1,
            phase: lgui_core::core::RenderPhase::Content,
        },
    );
    unsafe {
        let _ = GdiFlush();
    }
    assert_eq!(&output.pixels()[offset..offset + 3], &[255; 3]);
    drop(output);
    drop(renderer);
    unsafe {
        let _ = DeleteDC(dc);
    }
}
