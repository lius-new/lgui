use std::ops::Range;

use crate::core::UiRect;

use super::{TextAffinity, TextCluster, TextDirection, TextHit, TextLineMetrics};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TextCaret {
    index: usize,
    affinity: TextAffinity,
    rect: UiRect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    pub width: f32,
    pub height: f32,
    pub did_exceed_max_lines: bool,
    lines: Vec<TextLineMetrics>,
    clusters: Vec<TextCluster>,
    carets: Vec<TextCaret>,
}

impl TextLayout {
    pub fn new(
        width: f32,
        height: f32,
        did_exceed_max_lines: bool,
        lines: Vec<TextLineMetrics>,
        clusters: Vec<TextCluster>,
        carets: Vec<(usize, TextAffinity, UiRect)>,
    ) -> Self {
        Self {
            width,
            height,
            did_exceed_max_lines,
            lines,
            clusters,
            carets: carets
                .into_iter()
                .map(|(index, affinity, rect)| TextCaret {
                    index,
                    affinity,
                    rect,
                })
                .collect(),
        }
    }

    pub fn lines(&self) -> &[TextLineMetrics] {
        &self.lines
    }

    pub fn clusters(&self) -> &[TextCluster] {
        &self.clusters
    }

    pub fn caret_rect(&self, index: usize) -> Option<UiRect> {
        self.caret_rect_with_affinity(index, TextAffinity::Downstream)
            .or_else(|| {
                self.carets
                    .iter()
                    .find(|caret| caret.index == index)
                    .map(|c| c.rect)
            })
    }

    pub fn caret_rect_with_affinity(&self, index: usize, affinity: TextAffinity) -> Option<UiRect> {
        self.carets
            .iter()
            .find(|caret| caret.index == index && caret.affinity == affinity)
            .map(|caret| caret.rect)
    }

    pub fn selection_rects(&self, range: Range<usize>) -> Vec<UiRect> {
        if range.start >= range.end {
            return Vec::new();
        }
        let mut rects = Vec::<UiRect>::new();
        for cluster in self
            .clusters
            .iter()
            .filter(|cluster| cluster.range.start < range.end && cluster.range.end > range.start)
        {
            if let Some(last) = rects.last_mut() {
                let same_line = (last.top - cluster.bounds.top).abs() < 0.5
                    && (last.bottom - cluster.bounds.bottom).abs() < 0.5;
                let adjacent = (last.right - cluster.bounds.left).abs() < 1.0
                    || (cluster.bounds.right - last.left).abs() < 1.0;
                if same_line && adjacent {
                    last.left = last.left.min(cluster.bounds.left);
                    last.right = last.right.max(cluster.bounds.right);
                    continue;
                }
            }
            rects.push(cluster.bounds);
        }
        rects
    }

    pub fn previous_cluster_boundary(&self, index: usize) -> usize {
        self.clusters
            .iter()
            .flat_map(|cluster| [cluster.range.start, cluster.range.end])
            .chain(
                self.lines
                    .iter()
                    .flat_map(|line| [line.range.start, line.range.end]),
            )
            .filter(|boundary| *boundary < index)
            .max()
            .unwrap_or(0)
    }

    pub fn next_cluster_boundary(&self, index: usize) -> usize {
        self.clusters
            .iter()
            .flat_map(|cluster| [cluster.range.start, cluster.range.end])
            .chain(
                self.lines
                    .iter()
                    .flat_map(|line| [line.range.start, line.range.end]),
            )
            .filter(|boundary| *boundary > index)
            .min()
            .unwrap_or(index)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> TextHit {
        let Some(line) =
            self.lines.iter().min_by(|left, right| {
                distance_to_axis(y, left.bounds.top, left.bounds.bottom)
                    .total_cmp(&distance_to_axis(y, right.bounds.top, right.bounds.bottom))
            })
        else {
            return TextHit::default();
        };
        let mut clusters = self
            .clusters
            .iter()
            .filter(|cluster| {
                cluster.range.start >= line.range.start && cluster.range.end <= line.range.end
            })
            .peekable();
        let Some(first) = clusters.peek().cloned() else {
            return TextHit {
                index: line.range.start,
                affinity: TextAffinity::Downstream,
                inside: rect_contains(line.bounds, x, y),
            };
        };
        let mut nearest = first;
        let mut nearest_distance = distance_to_axis(x, first.bounds.left, first.bounds.right);
        for cluster in clusters {
            let distance = distance_to_axis(x, cluster.bounds.left, cluster.bounds.right);
            if distance < nearest_distance {
                nearest = cluster;
                nearest_distance = distance;
            }
        }
        let midpoint = (nearest.bounds.left + nearest.bounds.right) * 0.5;
        let leading_half = x < midpoint;
        let index = match nearest.direction {
            TextDirection::RightToLeft if leading_half => nearest.range.end,
            TextDirection::RightToLeft => nearest.range.start,
            _ if leading_half => nearest.range.start,
            _ => nearest.range.end,
        };
        TextHit {
            index,
            affinity: if leading_half {
                TextAffinity::Downstream
            } else {
                TextAffinity::Upstream
            },
            inside: rect_contains(line.bounds, x, y),
        }
    }
}

fn distance_to_axis(value: f32, start: f32, end: f32) -> f32 {
    if value < start {
        start - value
    } else if value > end {
        value - end
    } else {
        0.0
    }
}

fn rect_contains(rect: UiRect, x: f32, y: f32) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}
