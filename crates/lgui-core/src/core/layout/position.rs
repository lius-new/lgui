use crate::core::{Align, Dimension, HostTree, Point, Size, UiId, UiNode, UiRect};
use std::{collections::HashSet, sync::Arc};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AnchorSide {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Position {
    #[default]
    Flow,
    Absolute(Point),
    Layer {
        horizontal: Align,
        vertical: Align,
        offset: Point,
    },
    Anchor {
        target: Arc<str>,
        side: AnchorSide,
        gap: Dimension,
        offset: Point,
        flip: bool,
    },
    Sticky {
        top: Dimension,
        left: Dimension,
    },
}
impl Position {
    pub fn anchor(target: impl Into<Arc<str>>, side: AnchorSide) -> Self {
        Self::Anchor {
            target: target.into(),
            side,
            gap: Dimension::Points(0.0),
            offset: Point::default(),
            flip: true,
        }
    }
    pub(crate) fn participates(&self) -> bool {
        matches!(self, Self::Flow | Self::Sticky { .. })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapsed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Overflow {
    #[default]
    Visible,
    Clip,
    Ellipsis,
    Collapse(Size),
}
impl Overflow {
    pub(crate) fn clips(self) -> bool {
        matches!(self, Self::Clip | Self::Ellipsis)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionError {
    MissingAnchor(Arc<str>),
    AnchorCycle,
}
fn align(start: f32, extent: f32, size: f32, alignment: Align) -> f32 {
    match alignment {
        Align::Center => start + (extent - size) / 2.0,
        Align::End => start + extent - size,
        _ => start,
    }
}
fn value(d: Dimension, extent: f32) -> Option<f32> {
    match d {
        Dimension::Points(v) => Some(if v.is_finite() { v } else { 0.0 }),
        Dimension::Percent(v) => Some(extent * super::model::length(v)),
        _ => None,
    }
}
fn anchor_rect(target: UiRect, sz: Size, side: AnchorSide, gap: f32) -> UiRect {
    let (x, y) = match side {
        AnchorSide::Bottom => (target.left, target.bottom + gap),
        AnchorSide::Top => (target.left, target.top - sz.height - gap),
        AnchorSide::Left => (target.left - sz.width - gap, target.top),
        AnchorSide::Right => (target.right + gap, target.top),
    };
    UiRect::new(x, y, x + sz.width, y + sz.height)
}
fn clamp(r: UiRect, v: UiRect) -> UiRect {
    let x = r.left.clamp(v.left, (v.right - r.width()).max(v.left));
    let y = r.top.clamp(v.top, (v.bottom - r.height()).max(v.top));
    UiRect::new(x, y, x + r.width(), y + r.height())
}
pub(super) fn apply(tree: &mut HostTree, viewport: UiRect) -> (HashSet<UiId>, Vec<PositionError>) {
    let ids: Vec<_> = tree.nodes().iter().map(|n| n.id.clone()).collect();
    let mut changed = HashSet::new();
    let mut errors = Vec::new();
    for id in &ids {
        let Some(node) = tree.node(id).cloned() else {
            continue;
        };
        if node.visibility == Visibility::Collapsed {
            continue;
        }
        let original = node.position_origin.unwrap_or(node.layout_rect);
        if let Some(n) = tree.node_mut(id) {
            n.position_origin = Some(original);
        }
        let parent = node.parent.as_ref().and_then(|id| tree.node(id));
        let bounds = parent.map(super::measure::content_rect).unwrap_or(viewport);
        let sz = super::measure::rect_size(node.layout_rect);
        let next = match &node.position {
            Position::Flow | Position::Anchor { .. } => continue,
            Position::Absolute(p) => UiRect::new(
                bounds.left + p.x,
                bounds.top + p.y,
                bounds.left + p.x + sz.width,
                bounds.top + p.y + sz.height,
            ),
            Position::Layer {
                horizontal,
                vertical,
                offset,
            } => {
                let mut sz = sz;
                if *horizontal == Align::Stretch {
                    sz.width = bounds.width().max(0.0);
                }
                if *vertical == Align::Stretch {
                    sz.height = bounds.height().max(0.0);
                }
                if let Some(s) = node.size_constraints {
                    // Stretch allocates the available extent; explicit dimensions still win.
                    sz = s.resolve(super::measure::rect_size(bounds), sz, sz);
                }
                let x = align(bounds.left, bounds.width(), sz.width, *horizontal) + offset.x;
                let y = align(bounds.top, bounds.height(), sz.height, *vertical) + offset.y;
                UiRect::new(x, y, x + sz.width, y + sz.height)
            }
            Position::Sticky { top, left } => {
                let mut scroll = parent;
                while scroll.is_some_and(|n| n.clip_rect.is_none()) {
                    scroll = scroll
                        .and_then(|n| n.parent.as_ref())
                        .and_then(|p| tree.node(p));
                }
                let bounds = scroll.and_then(|n| n.clip_rect).unwrap_or(bounds);
                let offset = tree.ancestor_content_offset(&node);
                let x = left.to_owned();
                let y = top.to_owned();
                let x = value(x, bounds.width()).map_or(original.left, |v| {
                    original
                        .left
                        .max(bounds.left + v - offset.0)
                        .min((bounds.right - offset.0 - sz.width).max(bounds.left - offset.0))
                });
                let y = value(y, bounds.height()).map_or(original.top, |v| {
                    original
                        .top
                        .max(bounds.top + v - offset.1)
                        .min((bounds.bottom - offset.1 - sz.height).max(bounds.top - offset.1))
                });
                UiRect::new(x, y, x + sz.width, y + sz.height)
            }
        };
        move_node(tree, id, next, &mut changed);
    }
    fn resolve(
        tree: &mut HostTree,
        id: &UiId,
        viewport: UiRect,
        active: &mut HashSet<UiId>,
        done: &mut HashSet<UiId>,
        changed: &mut HashSet<UiId>,
    ) -> Result<(), PositionError> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.clone()) {
            return Err(PositionError::AnchorCycle);
        }
        let node = tree.node(id).cloned().unwrap();
        if let Position::Anchor {
            target,
            side,
            gap,
            offset,
            flip,
        } = &node.position
        {
            let target_id = tree
                .public_internal_id(target)
                .ok_or_else(|| PositionError::MissingAnchor(target.clone()))?;
            if matches!(
                tree.node(&target_id).unwrap().position,
                Position::Anchor { .. }
            ) {
                resolve(tree, &target_id, viewport, active, done, changed)?;
            }
            let target = tree.node(&target_id).unwrap();
            let trans = tree.ancestor_content_offset(target);
            let bounds = target.layout_rect.translate(trans.0, trans.1);
            let sz = super::measure::rect_size(node.layout_rect);
            let gap = value(*gap, viewport.width()).unwrap_or(0.0);
            let mut next = anchor_rect(bounds, sz, *side, gap).translate(offset.x, offset.y);
            if *flip && !fits_side(next, viewport, *side) {
                let opposite = match side {
                    AnchorSide::Bottom => AnchorSide::Top,
                    AnchorSide::Top => AnchorSide::Bottom,
                    AnchorSide::Left => AnchorSide::Right,
                    AnchorSide::Right => AnchorSide::Left,
                };
                let alternate =
                    anchor_rect(bounds, sz, opposite, gap).translate(offset.x, offset.y);
                if fits_side(alternate, viewport, opposite) {
                    next = alternate;
                }
            }
            next = clamp(next, viewport);
            let own = tree.ancestor_content_offset(&node);
            move_node(tree, id, next.translate(-own.0, -own.1), changed);
        }
        active.remove(id);
        done.insert(id.clone());
        Ok(())
    }
    let mut done = HashSet::new();
    for id in ids {
        if matches!(tree.node(&id).unwrap().position, Position::Anchor { .. })
            && let Err(e) = resolve(
                tree,
                &id,
                viewport,
                &mut HashSet::new(),
                &mut done,
                &mut changed,
            )
        {
            errors.push(e);
        }
    }
    (changed, errors)
}
fn move_node(tree: &mut HostTree, id: &UiId, rect: UiRect, changed: &mut HashSet<UiId>) {
    if tree.node(id).is_some_and(|n| n.layout_rect != rect) {
        fn collect(t: &HostTree, id: &UiId, out: &mut HashSet<UiId>) {
            out.insert(id.clone());
            if let Some(n) = t.node(id) {
                for c in &n.children {
                    collect(t, c, out);
                }
            }
        }
        collect(tree, id, changed);
        let origin = tree.node(id).and_then(|n| n.position_origin);
        super::layout::place_subtree(tree, id, rect);
        if let Some(n) = tree.node_mut(id) {
            n.position_origin = origin;
        }
    }
}
fn fits_side(r: UiRect, v: UiRect, side: AnchorSide) -> bool {
    match side {
        AnchorSide::Bottom => r.bottom <= v.bottom,
        AnchorSide::Top => r.top >= v.top,
        AnchorSide::Left => r.left >= v.left,
        AnchorSide::Right => r.right <= v.right,
    }
}

impl HostTree {
    pub fn is_visible(&self, id: &UiId) -> bool {
        if self.node(id).is_none() {
            return false;
        }
        let mut current = self.node(id);
        while let Some(n) = current {
            if n.visibility != Visibility::Visible || n.overflow_hidden {
                return false;
            }
            current = n.parent.as_ref().and_then(|p| self.node(p));
        }
        true
    }
    pub(crate) fn is_interactive(&self, id: &UiId) -> bool {
        if !self.is_visible(id) {
            return false;
        }
        let mut current = self.node(id);
        while let Some(n) = current {
            if n.inert {
                return false;
            }
            current = n.parent.as_ref().and_then(|p| self.node(p));
        }
        true
    }
    pub(crate) fn paint_children<'a>(&'a self, n: &UiNode) -> Vec<&'a crate::core::UiNode> {
        let mut children: Vec<_> = n.children.iter().filter_map(|id| self.node(id)).collect();
        children.sort_by_key(|n| n.z_index);
        children
    }
}
