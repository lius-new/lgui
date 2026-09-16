use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn checkmark_is_a_continuous_rising_tick_inside_the_checkbox_bounds() {
    let rect = UiRect::new(10.0, 20.0, 28.0, 38.0);
    let checkmark = checkmark(
        &UiId::owned("checkbox"),
        rect,
        RenderPhase::Content,
        Stroke::new(Color(0xFFFFFF), 2.0, 0xFF),
    );
    let commands = checkmark.node().path.as_ref().unwrap().commands();
    let [UiPathCommand::MoveTo(start), UiPathCommand::LineTo(bend), UiPathCommand::LineTo(end)] =
        commands
    else {
        panic!("checkbox checkmark must be one continuous three-point path");
    };

    for point in [start, bend, end] {
        assert!(rect.contains(*point));
    }
    assert!(start.x < bend.x && bend.x < end.x);
    assert!(start.y < bend.y);
    assert!(end.y < bend.y);
}

#[test]
fn checkbox_reports_the_opposite_controlled_value() {
    assert!(!next_checked(true));
    assert!(next_checked(false));

    let reported = Arc::new(AtomicBool::new(false));
    let next = Arc::clone(&reported);
    let checkbox = checkbox(UiRect::new(0.0, 0.0, 16.0, 16.0), false, move |checked| {
        next.store(checked, Ordering::SeqCst);
    });
    let mut context = UiEventContext::new(
        crate::application::ApplicationContext::empty(crate::memory::test_memory_options()),
        crate::application::WindowId::new("test"),
    );
    (checkbox.on_change)(&mut context, next_checked(checkbox.checked));
    assert!(reported.load(Ordering::SeqCst));
}
