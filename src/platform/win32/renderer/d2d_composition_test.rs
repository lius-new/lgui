use super::*;

#[test]
fn exact_viewport_damage_uses_the_full_present_path() {
    let viewport = PhysicalRect::new(0, 0, 1280, 720);

    assert!(damage_is_full(viewport, &[viewport]));
}

#[test]
fn partial_or_split_damage_keeps_the_dirty_present_path() {
    let viewport = PhysicalRect::new(0, 0, 1280, 720);

    assert!(!damage_is_full(
        viewport,
        &[PhysicalRect::new(12, 20, 240, 180)]
    ));
    assert!(!damage_is_full(
        viewport,
        &[
            PhysicalRect::new(0, 0, 640, 720),
            PhysicalRect::new(640, 0, 1280, 720),
        ]
    ));
}
