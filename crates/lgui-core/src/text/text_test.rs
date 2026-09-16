use super::*;
use crate::core::UiRect;

struct FixedTextSystem;

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
