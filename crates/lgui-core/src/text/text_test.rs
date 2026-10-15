use super::*;
use crate::core::UiRect;

struct FixedTextSystem;

#[test]
fn caret_queries_preserve_affinity_and_provider_order_for_unsorted_input() {
    let a = UiRect::new(10.0, 0.0, 10.0, 20.0);
    let b = UiRect::new(30.0, 0.0, 30.0, 20.0);
    let c = UiRect::new(40.0, 0.0, 40.0, 20.0);
    let layout = TextLayout::new(
        40.0,
        20.0,
        false,
        vec![],
        vec![],
        vec![
            (100, TextAffinity::Upstream, c),
            (2, TextAffinity::Upstream, a),
            (2, TextAffinity::Downstream, b),
            (2, TextAffinity::Downstream, c),
        ],
    );
    assert_eq!(layout.caret_rect(2), Some(b));
    assert_eq!(
        layout.caret_rect_with_affinity(2, TextAffinity::Upstream),
        Some(a)
    );
    assert_eq!(layout.caret_rect(100), Some(c));
    assert_eq!(layout.caret_rect(3), None);
    assert_eq!(layout.caret_rect(usize::MAX), None);
}

#[test]
fn width_measurement_does_not_build_editing_geometry() {
    struct MeasureOnly;
    impl TextSystem for MeasureOnly {
        fn measure(&self, _: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
            Some(TextMetrics { width: 42.0 })
        }
        fn layout(&self, _: &TextLayoutRequest<'_>) -> Option<TextLayout> {
            panic!("width measurement must not request caret or cluster geometry")
        }
    }
    let _guard = install_text_system(TextSystemHandle::new(MeasureOnly));
    assert_eq!(
        measure_width("content", UiRect::new(0.0, 0.0, 100.0, 20.0), 12.0, 400),
        Some(42.0)
    );
}

impl TextSystem for FixedTextSystem {
    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
        Some(TextMetrics {
            width: request.text.len() as f32 * 3.0,
        })
    }

    fn layout(&self, request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        let width = request.text.len() as f32 * 3.0;
        Some(TextLayout::new(
            width,
            10.0,
            false,
            vec![TextLineMetrics {
                range: 0..request.text.chars().count(),
                bounds: UiRect::new(0.0, 0.0, width, 10.0),
                baseline: 8.0,
                hard_break: false,
            }],
            Vec::new(),
            Vec::new(),
        ))
    }
}

#[test]
fn text_system_capability_is_scoped_and_restored() {
    let bounds = UiRect::new(0.0, 0.0, 100.0, 20.0);
    assert_eq!(measure_width("abc", bounds, -14.0, 400), None);
    {
        let _guard = install_text_system(TextSystemHandle::new(FixedTextSystem));
        assert_eq!(measure_width("abc", bounds, -14.0, 400), Some(9.0));
    }
    assert_eq!(measure_width("abc", bounds, -14.0, 400), None);
}

#[test]
fn selection_merges_adjacent_clusters_but_preserves_visual_runs() {
    let layout = TextLayout::new(
        30.0,
        10.0,
        false,
        vec![TextLineMetrics {
            range: 0..3,
            bounds: UiRect::new(0.0, 0.0, 30.0, 10.0),
            baseline: 8.0,
            hard_break: false,
        }],
        vec![
            TextCluster {
                range: 0..1,
                bounds: UiRect::new(0.0, 0.0, 10.0, 10.0),
                direction: TextDirection::LeftToRight,
            },
            TextCluster {
                range: 1..2,
                bounds: UiRect::new(10.0, 0.0, 20.0, 10.0),
                direction: TextDirection::LeftToRight,
            },
            TextCluster {
                range: 2..3,
                bounds: UiRect::new(40.0, 0.0, 50.0, 10.0),
                direction: TextDirection::RightToLeft,
            },
        ],
        Vec::new(),
    );
    assert_eq!(
        layout.selection_rects(0..3),
        vec![
            UiRect::new(0.0, 0.0, 20.0, 10.0),
            UiRect::new(40.0, 0.0, 50.0, 10.0)
        ]
    );
}
