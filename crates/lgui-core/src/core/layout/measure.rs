use std::{borrow::Cow, collections::HashMap};

use super::{Dimension, HostTree, Size, UiId, UiRect};
use crate::core::{Color, TextStyle, UiNode};

pub(super) type MeasurementCache = HashMap<(UiId, Size), Size>;
pub(super) type IntrinsicInput = (Option<Cow<'static, str>>, Option<TextStyle>);

pub(super) fn intrinsic_input(node: &UiNode) -> Option<IntrinsicInput> {
    if !node
        .size_constraints
        .is_some_and(|sizing| sizing.measures_content())
    {
        return None;
    }
    let style = node.text_style.map(|mut style| {
        // Paint-only changes must not invalidate content measurements.
        style.color = Color::WHITE;
        style.alpha = 255;
        style
    });
    Some((node.text.clone(), style))
}

pub(super) fn desired_size(
    tree: &HostTree,
    id: &UiId,
    available: Size,
    cache: &mut MeasurementCache,
) -> Size {
    let key = (id.clone(), available.normalized());
    if let Some(size) = cache.get(&key) {
        return *size;
    }
    let Some(node) = tree.node(id) else {
        return Size::default();
    };
    if node.visibility == crate::core::Visibility::Collapsed || collapses(node, available) {
        return Size::default();
    }
    let preferred = rect_size(node.layout_rect);
    let size = match node.size_constraints {
        None => preferred,
        Some(sizing) => {
            let intrinsic = if sizing.measures_content() {
                let initial = sizing.resolve(available, preferred, preferred);
                // Declared/definite widths constrain wrapping. Content widths use
                // the parent's available width, subject to the same min/max bounds
                // as the final box, so measured height matches painted wrapping.
                let width = match sizing.width {
                    Dimension::Auto | Dimension::Points(_) | Dimension::Percent(_) => initial.width,
                    Dimension::Content => {
                        let min = sizing.min.normalized().width;
                        let max = sizing
                            .max
                            .map_or(f32::MAX, |max| max.normalized().width.max(min));
                        available.width.clamp(min, max)
                    }
                };
                measure_content(
                    tree,
                    node,
                    Size::new(width, available.height),
                    preferred,
                    cache,
                )
            } else {
                preferred
            };
            sizing.resolve(available, preferred, intrinsic)
        }
    };
    cache.insert(key, size);
    size
}

pub(super) fn measure_content(
    tree: &HostTree,
    node: &UiNode,
    available: Size,
    fallback: Size,
    cache: &mut MeasurementCache,
) -> Size {
    if let (Some(text), Some(style)) = (&node.text, node.text_style) {
        let bounds = UiRect::new(0.0, 0.0, available.width, available.height);
        let request = crate::text::TextLayoutRequest::for_style(text, bounds, &style);
        if let Some(layout) = crate::text::layout(&request) {
            return Size::new(layout.width, layout.height).normalized();
        }
        if let Some(width) = crate::text::measure_width(text, bounds, style.height, style.weight) {
            return Size::new(width, fallback.height.max(style.height)).normalized();
        }
        // Headless callers can supply declared geometry when no text system is installed.
        return fallback;
    }
    if node.children.is_empty() {
        return fallback;
    }
    if let Some(size) = super::containers::intrinsic(tree, node, available, cache) {
        return size;
    }
    let mut size = Size::default();
    for child_id in &node.children {
        if let Some(child) = tree.node(child_id) {
            let child_size = desired_size(tree, child_id, available, cache);
            size.width = size
                .width
                .max(child.layout_rect.left - node.layout_rect.left + child_size.width);
            size.height = size
                .height
                .max(child.layout_rect.top - node.layout_rect.top + child_size.height);
        }
    }
    size.normalized()
}

pub(super) fn rect_size(rect: UiRect) -> Size {
    Size::new(rect.width(), rect.height()).normalized()
}

pub(super) fn content_rect(node: &UiNode) -> UiRect {
    node.layout_rect.inset(node.layout.padding())
}
pub(super) fn collapses(node: &UiNode, available: Size) -> bool {
    if let crate::core::Overflow::Collapse(min) = node.overflow {
        let min = min.normalized();
        available.width < min.width || available.height < min.height
    } else {
        false
    }
}
