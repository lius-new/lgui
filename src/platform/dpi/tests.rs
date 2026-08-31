use super::*;

fn work(width: i32, height: i32) -> WorkArea {
    WorkArea {
        rect: PhysicalRect::new(0, 0, width, height),
    }
}

#[test]
fn automatic_scale_fits_the_work_area() {
    let context = ScaleContext::resolve(
        144,
        work(1920, 1040),
        Size::new(1432.0, 860.0),
        ScalePreference::Auto,
    );

    assert!(context.scale.factor() < 1.5);
    assert!(context.physical_window_size.width <= 1920);
    assert!(context.physical_window_size.height <= 1040);
}

#[test]
fn manual_scale_is_relative_to_monitor_dpi() {
    let context = ScaleContext::resolve(
        144,
        work(2560, 1400),
        Size::new(1432.0, 860.0),
        ScalePreference::Multiplier(0.8),
    );

    assert_eq!(context.scale, UiScale::new(1.2));
}

#[test]
fn very_small_work_area_switches_to_compact_viewport() {
    let context = ScaleContext::resolve(
        120,
        work(1024, 700),
        Size::new(1432.0, 860.0),
        ScalePreference::Multiplier(1.0),
    );

    assert!(context.compact);
    assert!(context.physical_window_size.width < 1024);
    assert!(context.physical_window_size.height < 700);
}

#[test]
fn clamp_origin_preserves_negative_monitor_coordinates() {
    let context = ScaleContext::resolve(
        96,
        WorkArea {
            rect: PhysicalRect::new(-1920, 0, 0, 1080),
        },
        Size::new(800.0, 600.0),
        ScalePreference::Multiplier(1.0),
    );

    assert_eq!(
        context.clamp_origin(PhysicalPoint::new(-2200, -100), PhysicalSize::new(800, 600)),
        PhysicalPoint::new(-1920, 0)
    );
}

#[test]
fn monitor_metrics_include_work_area_and_preference() {
    let primary = ScaleContext::resolve(
        96,
        work(2560, 1400),
        Size::new(1432.0, 860.0),
        ScalePreference::Auto,
    );
    let secondary = ScaleContext::resolve(
        96,
        WorkArea {
            rect: PhysicalRect::new(-1920, 0, 0, 1040),
        },
        Size::new(1432.0, 860.0),
        ScalePreference::Auto,
    );
    let manual = ScaleContext::resolve(
        96,
        work(2560, 1400),
        Size::new(1432.0, 860.0),
        ScalePreference::Multiplier(1.0),
    );

    assert!(!primary.same_monitor_metrics(secondary));
    assert!(!primary.same_monitor_metrics(manual));
    assert!(primary.same_monitor_metrics(primary));
}

#[test]
fn auto_scale_targets_ninety_percent_on_a_1080p_work_area() {
    let context = ScaleContext::resolve(
        96,
        work(1920, 1032),
        Size::new(1432.0, 860.0),
        ScalePreference::Auto,
    );

    assert!((context.scale.factor() - 0.9).abs() < 0.001);
    assert_eq!(context.physical_window_size, PhysicalSize::new(1289, 774));
}

#[test]
fn explicit_half_scale_is_allowed_below_the_auto_minimum() {
    let context = ScaleContext::resolve(
        96,
        work(1920, 1032),
        Size::new(1432.0, 860.0),
        ScalePreference::Multiplier(0.5),
    );

    assert_eq!(context.scale, UiScale::new(0.5));
    assert_eq!(context.physical_window_size, PhysicalSize::new(716, 430));
}
