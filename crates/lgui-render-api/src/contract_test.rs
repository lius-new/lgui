use super::*;

#[test]
fn frame_stats_use_damage_or_the_full_viewport() {
    let viewport = PhysicalRect::new(0, 0, 100, 80);
    let damage = [
        PhysicalRect::new(0, 0, 10, 20),
        PhysicalRect::new(50, 40, 60, 50),
    ];
    let dirty = FrameInfo::new(
        viewport,
        &damage,
        UiScale::ONE,
        FrameReason::SceneChange,
        false,
    );
    let full = FrameInfo::new(viewport, &damage, UiScale::ONE, FrameReason::Resize, true);

    assert_eq!(
        RenderStats::for_frame(&dirty),
        RenderStats {
            painted_rects: 2,
            painted_pixels: 300,
        }
    );
    assert_eq!(
        RenderStats::for_frame(&full),
        RenderStats {
            painted_rects: 1,
            painted_pixels: 8_000,
        }
    );
}
