use super::{Align, Axis, Dimension, EdgeInsets, Size};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

pub(crate) fn length(v: f32) -> f32 {
    if v.is_finite() { v.max(0.0) + 0.0 } else { 0.0 }
}
fn bits(v: f32) -> u32 {
    length(v).to_bits()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug)]
pub struct FlexLayout {
    pub axis: Axis,
    pub gap: f32,
    pub padding: EdgeInsets,
    pub align: Align,
    pub justify: Justify,
    pub wrap: bool,
}
impl FlexLayout {
    /// Measure already measured children using the container's own wrap rules.
    pub fn measure(self, available: Size, children: &[Size]) -> Size {
        use crate::core::{HostTree, SizeConstraints, UiId, UiNode, UiNodeKind, UiRect};
        let id = UiId::new("measure.flex");
        let mut tree = HostTree::new();
        let sizing = if self.axis == Axis::Horizontal {
            SizeConstraints::new(Dimension::Points(available.width), Dimension::Content)
        } else {
            SizeConstraints::new(Dimension::Content, Dimension::Points(available.height))
        };
        tree.push(
            UiNode::new(id.clone(), UiNodeKind::Group, UiRect::default())
                .layout(LayoutSpec::Flex(self))
                .size_constraints(sizing),
        );
        for (i, size) in children.iter().enumerate() {
            tree.push(
                UiNode::new(
                    id.child(i.to_string()),
                    UiNodeKind::Group,
                    UiRect::new(0.0, 0.0, size.width, size.height),
                )
                .parent(id.clone()),
            );
        }
        super::measure::desired_size(
            &tree,
            &id,
            available,
            &mut super::measure::MeasurementCache::new(),
        )
    }
    pub const fn new(axis: Axis) -> Self {
        Self {
            axis,
            gap: 0.0,
            padding: EdgeInsets::ZERO,
            align: Align::Start,
            justify: Justify::Start,
            wrap: false,
        }
    }
    fn key(self) -> (Axis, u32, EdgeInsets, Align, Justify, bool) {
        (
            self.axis,
            bits(self.gap),
            self.padding,
            self.align,
            self.justify,
            self.wrap,
        )
    }
}
impl Default for FlexLayout {
    fn default() -> Self {
        Self::new(Axis::Horizontal)
    }
}
impl PartialEq for FlexLayout {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for FlexLayout {}
impl Hash for FlexLayout {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.key().hash(h);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FlexItem {
    pub grow: f32,
    pub shrink: f32,
    pub basis: Dimension,
    pub align_self: Option<Align>,
}
impl Default for FlexItem {
    fn default() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            basis: Dimension::Auto,
            align_self: None,
        }
    }
}
impl FlexItem {
    pub fn grow(grow: f32) -> Self {
        Self {
            grow,
            ..Self::default()
        }
    }
    fn key(self) -> (u32, u32, Dimension, Option<Align>) {
        (
            bits(self.grow),
            bits(self.shrink),
            self.basis,
            self.align_self,
        )
    }
}
impl PartialEq for FlexItem {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for FlexItem {}
impl Hash for FlexItem {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.key().hash(h);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DockSide {
    Top,
    Bottom,
    Left,
    Right,
    #[default]
    Fill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridCell {
    pub column: Option<usize>,
    pub row: Option<usize>,
    pub column_span: usize,
    pub row_span: usize,
}
impl GridCell {
    pub const fn at(column: usize, row: usize) -> Self {
        Self {
            column: Some(column),
            row: Some(row),
            column_span: 1,
            row_span: 1,
        }
    }
    pub const fn span(mut self, columns: usize, rows: usize) -> Self {
        self.column_span = columns;
        self.row_span = rows;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub enum GridTrack {
    Points(f32),
    Content,
    Fraction(f32),
}
impl GridTrack {
    fn key(self) -> (u8, u32) {
        match self {
            Self::Points(v) => (0, bits(v)),
            Self::Content => (1, 0),
            Self::Fraction(v) => (2, bits(v)),
        }
    }
}
impl PartialEq for GridTrack {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for GridTrack {}
impl Hash for GridTrack {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.key().hash(h);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GridLayout {
    pub columns: Arc<[GridTrack]>,
    pub rows: Arc<[GridTrack]>,
    pub gap: Size,
    pub padding: EdgeInsets,
}
impl GridLayout {
    pub fn new(columns: impl Into<Arc<[GridTrack]>>) -> Self {
        Self {
            columns: columns.into(),
            rows: Arc::from([]),
            gap: Size::default(),
            padding: EdgeInsets::ZERO,
        }
    }
    pub fn rows(mut self, rows: impl Into<Arc<[GridTrack]>>) -> Self {
        self.rows = rows.into();
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum LayoutSpec {
    #[default]
    None,
    Flex(FlexLayout),
    Dock {
        padding: EdgeInsets,
        gap: Dimension,
    },
    Grid(GridLayout),
}
impl LayoutSpec {
    pub(crate) fn padding(&self) -> EdgeInsets {
        match self {
            Self::Flex(v) => v.padding,
            Self::Dock { padding, .. } => *padding,
            Self::Grid(v) => v.padding,
            Self::None => EdgeInsets::ZERO,
        }
    }
    pub(crate) fn is_container(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LayoutItem {
    pub flex: FlexItem,
    pub dock: DockSide,
    pub grid: GridCell,
}
