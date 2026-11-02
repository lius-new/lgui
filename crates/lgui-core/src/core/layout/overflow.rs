use crate::core::{HostTree, Overflow, UiId};
use unicode_segmentation::UnicodeSegmentation;
pub(super) fn apply(tree: &mut HostTree, ids: &std::collections::HashSet<UiId>) {
    for id in ids {
        let Some(node) = tree.node(id).cloned() else {
            continue;
        };
        let mut displayed = None;
        if node.overflow == Overflow::Ellipsis
            && let (Some(text), Some(style)) = (&node.text, node.text_style)
        {
            let width = node.layout_rect.width().max(0.0);
            let measure = |s: &str| {
                crate::text::measure_width(s, node.layout_rect, style.height, style.weight)
            };
            if measure(text).is_some_and(|w| w > width) {
                if measure("…").is_some_and(|w| w <= width) {
                    let stops: Vec<_> = text
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .chain(std::iter::once(text.len()))
                        .collect();
                    let (mut lo, mut hi) = (0, stops.len() - 1);
                    while lo < hi {
                        let mid = lo + (hi - lo).div_ceil(2);
                        if measure(&format!("{}…", &text[..stops[mid]])).is_some_and(|w| w <= width)
                        {
                            lo = mid;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    displayed = Some(std::borrow::Cow::Owned(format!("{}…", &text[..stops[lo]])));
                } else {
                    displayed = Some(std::borrow::Cow::Borrowed(""));
                }
            }
        }
        if let Some(n) = tree.node_mut(id) {
            n.display_text = displayed;
            if n.overflow.clips() {
                n.clip_rect = Some(n.layout_rect);
            }
        }
    }
}
