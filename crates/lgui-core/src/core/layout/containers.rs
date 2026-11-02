use super::measure::{MeasurementCache, desired_size, measure_content, rect_size};
use super::model::length;
use super::{
    Align, Axis, Dimension, DockSide, FlexLayout, GridLayout, GridTrack, HostTree, Justify,
    LayoutSpec, Size, SizeConstraints, UiId, UiRect,
};
use crate::core::UiNode;

fn main(size: Size, axis: Axis) -> f32 {
    if axis == Axis::Horizontal {
        size.width
    } else {
        size.height
    }
}
fn cross(size: Size, axis: Axis) -> f32 {
    if axis == Axis::Horizontal {
        size.height
    } else {
        size.width
    }
}
fn size(m: f32, c: f32, axis: Axis) -> Size {
    if axis == Axis::Horizontal {
        Size::new(m, c)
    } else {
        Size::new(c, m)
    }
}
fn bounded(node: &UiNode, value: f32, axis: Axis) -> f32 {
    let Some(s) = node.size_constraints else {
        return length(value);
    };
    let min = main(s.min.normalized(), axis);
    let max = s
        .max
        .map_or(f32::MAX, |v| main(v.normalized(), axis).max(min));
    length(value).clamp(min, max)
}

pub(super) fn arrange(
    tree: &HostTree,
    node: &UiNode,
    cache: &mut MeasurementCache,
) -> Vec<(UiId, UiRect)> {
    let bounds = node.layout_rect.inset(node.layout.padding());
    let available = rect_size(bounds);
    let children: Vec<_> = node
        .children
        .iter()
        .filter_map(|id| tree.node(id))
        .filter(|n| {
            n.position.participates()
                && n.visibility != crate::core::Visibility::Collapsed
                && !super::measure::collapses(n, available)
        })
        .collect();
    let measured: Vec<_> = children
        .iter()
        .map(|n| desired_size(tree, &n.id, available, cache))
        .collect();
    let rects = match &node.layout {
        LayoutSpec::None => return Vec::new(),
        LayoutSpec::Flex(spec) => flex(tree, &children, &measured, bounds, *spec, cache),
        LayoutSpec::Dock { gap, .. } => dock(
            &children,
            &measured,
            bounds,
            match gap {
                Dimension::Points(v) => length(*v),
                _ => 0.0,
            },
        ),
        LayoutSpec::Grid(spec) => grid(&children, &measured, bounds, spec),
    };
    children
        .into_iter()
        .zip(rects)
        .zip(measured)
        .map(|((n, mut r), intrinsic)| {
            if !matches!(node.layout, LayoutSpec::Flex(_))
                && let Some(sizing) = n.size_constraints
            {
                let sz = sizing.resolve(rect_size(r), rect_size(r), intrinsic);
                r = UiRect::new(r.left, r.top, r.left + sz.width, r.top + sz.height);
            }
            (n.id.clone(), r)
        })
        .collect()
}

fn flex(
    tree: &HostTree,
    nodes: &[&UiNode],
    measured: &[Size],
    bounds: UiRect,
    spec: FlexLayout,
    cache: &mut MeasurementCache,
) -> Vec<UiRect> {
    let available = rect_size(bounds);
    let extent = main(available, spec.axis);
    let gap = length(spec.gap);
    let base: Vec<f32> = nodes
        .iter()
        .zip(measured)
        .map(|(n, sz)| {
            bounded(
                n,
                match n.layout_item.flex.basis {
                    Dimension::Points(v) => v,
                    Dimension::Percent(v) => extent * length(v),
                    Dimension::Content => {
                        main(measure_content(tree, n, available, *sz, cache), spec.axis)
                    }
                    Dimension::Auto => main(*sz, spec.axis),
                },
                spec.axis,
            )
        })
        .collect();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (index, value) in base.iter().enumerate() {
        if spec.wrap && !lines.last().unwrap().is_empty() && used + gap + value > extent {
            lines.push(Vec::new());
            used = 0.0;
        }
        if !lines.last().unwrap().is_empty() {
            used += gap;
        }
        lines.last_mut().unwrap().push(index);
        used += value;
    }
    let mut rects = vec![UiRect::new(0.0, 0.0, 0.0, 0.0); nodes.len()];
    let mut cross_cursor = 0.0;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let mut widths: Vec<f32> = line.iter().map(|i| base[*i]).collect();
        let mut frozen = vec![false; line.len()];
        for _ in 0..=line.len() {
            let free =
                extent - gap * line.len().saturating_sub(1) as f32 - widths.iter().sum::<f32>();
            let weights: Vec<_> = line
                .iter()
                .enumerate()
                .map(|(j, i)| {
                    if frozen[j] {
                        0.0
                    } else if free >= 0.0 {
                        length(nodes[*i].layout_item.flex.grow)
                    } else {
                        length(nodes[*i].layout_item.flex.shrink) * base[*i]
                    }
                })
                .collect();
            let total: f32 = weights.iter().sum();
            if total <= 0.0 || free.abs() < 0.001 {
                break;
            }
            let mut hit_limit = false;
            for (j, i) in line.iter().enumerate() {
                if weights[j] == 0.0 {
                    continue;
                }
                let proposed = widths[j] + free * weights[j] / total;
                let next = bounded(nodes[*i], proposed, spec.axis);
                if (next - proposed).abs() > 0.001 {
                    frozen[j] = true;
                    hit_limit = true;
                }
                widths[j] = next;
            }
            if !hit_limit {
                break;
            }
        }
        let line_cross = if spec.wrap {
            line.iter()
                .map(|i| cross(measured[*i], spec.axis))
                .fold(0.0, f32::max)
        } else {
            cross(available, spec.axis)
        };
        let free =
            (extent - widths.iter().sum::<f32>() - gap * line.len().saturating_sub(1) as f32)
                .max(0.0);
        let n = line.len() as f32;
        let (mut cursor, extra) = match spec.justify {
            Justify::Start => (0.0, 0.0),
            Justify::Center => (free / 2.0, 0.0),
            Justify::End => (free, 0.0),
            Justify::SpaceBetween => (0.0, if n > 1.0 { free / (n - 1.0) } else { 0.0 }),
            Justify::SpaceAround => (free / (2.0 * n), free / n),
            Justify::SpaceEvenly => (free / (n + 1.0), free / (n + 1.0)),
        };
        for (j, i) in line.iter().enumerate() {
            let align = nodes[*i].layout_item.flex.align_self.unwrap_or(spec.align);
            let mut desired = size(widths[j], cross(measured[*i], spec.axis), spec.axis);
            if let Some(s) = nodes[*i].size_constraints {
                let cross_dim = if spec.axis == Axis::Horizontal {
                    s.height
                } else {
                    s.width
                };
                if align == Align::Stretch && matches!(cross_dim, Dimension::Auto) {
                    desired = size(widths[j], line_cross, spec.axis);
                }
                let constrained = if spec.axis == Axis::Horizontal {
                    SizeConstraints {
                        width: Dimension::Points(widths[j]),
                        ..s
                    }
                } else {
                    SizeConstraints {
                        height: Dimension::Points(widths[j]),
                        ..s
                    }
                };
                desired = constrained.resolve(available, desired, measured[*i]);
            } else if align == Align::Stretch {
                desired = size(widths[j], line_cross, spec.axis);
            }
            let offset = match align {
                Align::Center => (line_cross - cross(desired, spec.axis)) / 2.0,
                Align::End => line_cross - cross(desired, spec.axis),
                _ => 0.0,
            };
            let (x, y) = if spec.axis == Axis::Horizontal {
                (bounds.left + cursor, bounds.top + cross_cursor + offset)
            } else {
                (bounds.left + cross_cursor + offset, bounds.top + cursor)
            };
            rects[*i] = UiRect::new(x, y, x + desired.width, y + desired.height);
            cursor += widths[j] + gap + extra;
        }
        cross_cursor += line_cross + gap;
    }
    rects
}

fn dock(nodes: &[&UiNode], sizes: &[Size], mut remaining: UiRect, gap: f32) -> Vec<UiRect> {
    nodes
        .iter()
        .zip(sizes)
        .map(|(node, sz)| {
            // Explicit/minimum sizes may overflow. Preserve the docked edge;
            // clipping is the parent's policy, while remaining space saturates.
            let w = sz.width.max(0.0);
            let h = sz.height.max(0.0);
            match node.layout_item.dock {
                DockSide::Top => {
                    let r = UiRect::new(
                        remaining.left,
                        remaining.top,
                        remaining.right,
                        remaining.top + h,
                    );
                    remaining.top = (r.bottom + gap).min(remaining.bottom);
                    r
                }
                DockSide::Bottom => {
                    let r = UiRect::new(
                        remaining.left,
                        remaining.bottom - h,
                        remaining.right,
                        remaining.bottom,
                    );
                    remaining.bottom = (r.top - gap).max(remaining.top);
                    r
                }
                DockSide::Left => {
                    let r = UiRect::new(
                        remaining.left,
                        remaining.top,
                        remaining.left + w,
                        remaining.bottom,
                    );
                    remaining.left = (r.right + gap).min(remaining.right);
                    r
                }
                DockSide::Right => {
                    let r = UiRect::new(
                        remaining.right - w,
                        remaining.top,
                        remaining.right,
                        remaining.bottom,
                    );
                    remaining.right = (r.left - gap).max(remaining.left);
                    r
                }
                DockSide::Fill => remaining,
            }
        })
        .collect()
}

fn cell(node: &UiNode, index: usize, columns: usize, rows: usize) -> (usize, usize, usize, usize) {
    let c = node.layout_item.grid;
    let column = c.column.unwrap_or(index % columns).min(columns - 1);
    let row = c.row.unwrap_or(index / columns).min(rows - 1);
    (
        column,
        row,
        c.column_span.max(1).min(columns - column),
        c.row_span.max(1).min(rows - row),
    )
}

fn tracks(
    spec: &[GridTrack],
    count: usize,
    measured: &[(usize, usize, f32)],
    available: f32,
    gap: f32,
    fill: bool,
) -> Vec<f32> {
    let track = |i| spec.get(i).copied().unwrap_or(GridTrack::Content);
    let mut values: Vec<_> = (0..count)
        .map(|i| match track(i) {
            GridTrack::Points(v) => length(v),
            _ => 0.0,
        })
        .collect();
    for &(start, span, desired) in measured {
        let missing = (desired
            - values[start..start + span].iter().sum::<f32>()
            - gap * span.saturating_sub(1) as f32)
            .max(0.0);
        let flexible: Vec<_> = (start..start + span)
            .filter(|i| !matches!(track(*i), GridTrack::Points(_)))
            .collect();
        for i in &flexible {
            values[*i] += missing / flexible.len() as f32;
        }
    }
    if fill {
        let mut frozen: Vec<_> = (0..count)
            .map(|i| !matches!(track(i),GridTrack::Fraction(v) if v>0.0))
            .collect();
        for _ in 0..=count {
            let pool = (available
                - gap * count.saturating_sub(1) as f32
                - values
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| frozen[*i])
                    .map(|(_, v)| v)
                    .sum::<f32>())
            .max(0.0);
            let weight: f32 = (0..count)
                .filter(|i| !frozen[*i])
                .map(|i| {
                    if let GridTrack::Fraction(v) = track(i) {
                        length(v)
                    } else {
                        0.0
                    }
                })
                .sum();
            if weight == 0.0 {
                break;
            }
            let mut limited = false;
            for i in 0..count {
                if !frozen[i]
                    && let GridTrack::Fraction(v) = track(i)
                    && pool * length(v) / weight < values[i]
                {
                    frozen[i] = true;
                    limited = true;
                }
            }
            if !limited {
                for i in 0..count {
                    if !frozen[i]
                        && let GridTrack::Fraction(v) = track(i)
                    {
                        values[i] = pool * length(v) / weight;
                    }
                }
                break;
            }
        }
    }
    values
}

fn grid(nodes: &[&UiNode], sizes: &[Size], bounds: UiRect, spec: &GridLayout) -> Vec<UiRect> {
    let columns = spec.columns.len().max(1);
    let (cells, rows) = grid_cells(nodes, columns, spec.rows.len());
    let widths: Vec<_> = cells
        .iter()
        .zip(sizes)
        .map(|(&(c, _, span, _), s)| (c, span, s.width))
        .collect();
    let heights: Vec<_> = cells
        .iter()
        .zip(sizes)
        .map(|(&(_, r, _, span), s)| (r, span, s.height))
        .collect();
    let w = tracks(
        &spec.columns,
        columns,
        &widths,
        bounds.width(),
        length(spec.gap.width),
        true,
    );
    let h = tracks(
        &spec.rows,
        rows,
        &heights,
        bounds.height(),
        length(spec.gap.height),
        true,
    );
    cells
        .into_iter()
        .map(|(c, r, cs, rs)| {
            let x = bounds.left + w[..c].iter().sum::<f32>() + length(spec.gap.width) * c as f32;
            let y = bounds.top + h[..r].iter().sum::<f32>() + length(spec.gap.height) * r as f32;
            UiRect::new(
                x,
                y,
                x + w[c..c + cs].iter().sum::<f32>() + length(spec.gap.width) * (cs - 1) as f32,
                y + h[r..r + rs].iter().sum::<f32>() + length(spec.gap.height) * (rs - 1) as f32,
            )
        })
        .collect()
}

fn grid_cells(
    nodes: &[&UiNode],
    columns: usize,
    declared_rows: usize,
) -> (Vec<(usize, usize, usize, usize)>, usize) {
    let limit = declared_rows.max(nodes.len()).max(1);
    let mut occupied = std::collections::HashSet::new();
    let mut cells = Vec::new();
    let mut rows = declared_rows.max(1);
    for (i, n) in nodes.iter().enumerate() {
        let (mut c, mut r, cs, rs) = cell(n, i, columns, limit);
        if n.layout_item.grid.column.is_none() || n.layout_item.grid.row.is_none() {
            'find: for row in 0..limit {
                for col in 0..columns {
                    if n.layout_item
                        .grid
                        .column
                        .is_some_and(|v| v.min(columns - 1) != col)
                        || n.layout_item
                            .grid
                            .row
                            .is_some_and(|v| v.min(limit - 1) != row)
                    {
                        continue;
                    }
                    if col + cs <= columns
                        && row + rs <= limit
                        && (row..row + rs)
                            .all(|y| (col..col + cs).all(|x| !occupied.contains(&(x, y))))
                    {
                        c = col;
                        r = row;
                        break 'find;
                    }
                }
            }
        }
        for y in r..r + rs {
            for x in c..c + cs {
                occupied.insert((x, y));
            }
        }
        rows = rows.max(r + rs);
        cells.push((c, r, cs, rs));
    }
    (cells, rows)
}

pub(super) fn intrinsic(
    tree: &HostTree,
    node: &UiNode,
    available: Size,
    cache: &mut MeasurementCache,
) -> Option<Size> {
    let p = node.layout.padding();
    let inner = Size::new(
        available.width - p.left - p.right,
        available.height - p.top - p.bottom,
    )
    .normalized();
    let children: Vec<_> = node
        .children
        .iter()
        .filter_map(|id| tree.node(id))
        .filter(|n| {
            n.position.participates()
                && n.visibility != crate::core::Visibility::Collapsed
                && !super::measure::collapses(n, inner)
        })
        .collect();
    let measured: Vec<_> = children
        .iter()
        .map(|n| desired_size(tree, &n.id, inner, cache))
        .collect();
    let content = match &node.layout {
        LayoutSpec::None => return None,
        LayoutSpec::Flex(f) => {
            let mut m: f32 = 0.0;
            let mut c: f32 = 0.0;
            let mut total_cross: f32 = 0.0;
            let mut widest: f32 = 0.0;
            for (node, s) in children.iter().zip(measured) {
                let next = bounded(
                    node,
                    match node.layout_item.flex.basis {
                        Dimension::Points(v) => v,
                        Dimension::Percent(v) => main(inner, f.axis) * length(v),
                        Dimension::Content => {
                            main(measure_content(tree, node, inner, s, cache), f.axis)
                        }
                        Dimension::Auto => main(s, f.axis),
                    },
                    f.axis,
                );
                if f.wrap && m > 0.0 && m + length(f.gap) + next > main(inner, f.axis) {
                    widest = widest.max(m);
                    total_cross += c + length(f.gap);
                    m = 0.0;
                    c = 0.0;
                }
                if m > 0.0 {
                    m += length(f.gap);
                }
                m += next;
                c = c.max(cross(s, f.axis));
            }
            size(widest.max(m), total_cross + c, f.axis)
        }
        LayoutSpec::Grid(g) => {
            let columns = g.columns.len().max(1);
            let (cells, rows) = grid_cells(&children, columns, g.rows.len());
            let widths: Vec<_> = cells
                .iter()
                .zip(&measured)
                .map(|(&(c, _, span, _), s)| (c, span, s.width))
                .collect();
            let heights: Vec<_> = cells
                .iter()
                .zip(&measured)
                .map(|(&(_, r, _, span), s)| (r, span, s.height))
                .collect();
            Size::new(
                tracks(
                    &g.columns,
                    columns,
                    &widths,
                    inner.width,
                    length(g.gap.width),
                    false,
                )
                .iter()
                .sum::<f32>()
                    + length(g.gap.width) * (columns - 1) as f32,
                tracks(
                    &g.rows,
                    rows,
                    &heights,
                    inner.height,
                    length(g.gap.height),
                    false,
                )
                .iter()
                .sum::<f32>()
                    + length(g.gap.height) * (rows - 1) as f32,
            )
        }
        LayoutSpec::Dock { gap, .. } => {
            let gap = match gap {
                Dimension::Points(v) => length(*v),
                _ => 0.0,
            };
            let mut edges = Size::default();
            let mut fill = Size::default();
            for (i, (n, s)) in children.iter().zip(measured).enumerate() {
                let gap = if i + 1 < children.len() { gap } else { 0.0 };
                match n.layout_item.dock {
                    DockSide::Top | DockSide::Bottom => {
                        edges.height += s.height + gap;
                        fill.width = fill.width.max(s.width);
                    }
                    DockSide::Left | DockSide::Right => {
                        edges.width += s.width + gap;
                        fill.height = fill.height.max(s.height);
                    }
                    DockSide::Fill => {
                        fill.width = fill.width.max(s.width);
                        fill.height = fill.height.max(s.height);
                    }
                }
            }
            Size::new(edges.width + fill.width, edges.height + fill.height)
        }
    };
    Some(
        Size::new(
            content.width + p.left + p.right,
            content.height + p.top + p.bottom,
        )
        .normalized(),
    )
}
