use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::*;

#[test]
fn default_transform_is_identity() {
    let transform = LayerTransform::default();
    assert!(transform.is_identity());
    assert_eq!(transform.rotation_degrees_f32(), 0.0);
    assert_eq!((transform.scale_x(), transform.scale_y()), (1.0, 1.0));
    assert_eq!((transform.origin_x(), transform.origin_y()), (0.5, 0.5));
    assert_eq!(
        (transform.translation_x(), transform.translation_y()),
        (0.0, 0.0)
    );
}

#[test]
fn transform_values_are_normalized_and_stably_hashable() {
    let first = LayerTransform::identity()
        .rotation_degrees(450.0)
        .scale_xy(1.25, 0.75)
        .origin(-1.0, 2.0);
    let second = LayerTransform::identity()
        .rotation_degrees(90.0)
        .scale_xy(1.25, 0.75)
        .origin(0.0, 1.0);
    assert_eq!(first, second);
    assert_eq!(first.rotation_degrees_f32(), 90.0);
    assert_eq!((first.origin_x(), first.origin_y()), (0.0, 1.0));

    let hash = |value: LayerTransform| {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(first), hash(second));
}

#[test]
fn transformed_bounds_include_scaled_rotation() {
    let transform = LayerTransform::identity()
        .rotation_degrees(90.0)
        .scale_xy(2.0, 1.0);
    assert_eq!(
        transform.transformed_bounds(super::super::UiRect::new(10.0, 20.0, 30.0, 60.0)),
        super::super::UiRect::new(0.0, 20.0, 40.0, 60.0)
    );
}

#[test]
fn transformed_bounds_include_translation() {
    let transform = LayerTransform::identity().translation(120.5, -30.25);
    assert_eq!(
        transform.transformed_bounds(super::super::UiRect::new(0.0, 0.0, 80.0, 80.0)),
        super::super::UiRect::new(120.5, -30.25, 200.5, 49.75)
    );
}

#[test]
fn physical_projection_scales_only_translation() {
    let transform = LayerTransform::identity()
        .rotation_degrees(45.0)
        .scale(1.25)
        .translation(100.0, -20.0)
        .project_to_physical(super::super::UiScale::new(1.5));
    assert_eq!(transform.rotation_degrees_f32(), 45.0);
    assert_eq!((transform.scale_x(), transform.scale_y()), (1.25, 1.25));
    assert_eq!(
        (transform.translation_x(), transform.translation_y()),
        (150.0, -30.0)
    );
}
