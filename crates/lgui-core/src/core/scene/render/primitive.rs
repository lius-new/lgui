use super::{transform::command_signature_part, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderPhase {
    Background,
    Content,
    Overlay,
    Popup,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScenePrimitive {
    Rect {
        id: UiId,
        rect: UiRect,
        style: VisualStyle,
        phase: RenderPhase,
    },
    Ellipse {
        id: UiId,
        rect: UiRect,
        style: VisualStyle,
        phase: RenderPhase,
    },
    Text {
        id: UiId,
        rect: UiRect,
        text: Cow<'static, str>,
        style: TextStyle,
        phase: RenderPhase,
    },
    Custom {
        id: UiId,
        rect: UiRect,
        key: &'static str,
        // Style may contain caller animation state. Backends should treat Custom as immediate
        // paint unless the specific painter exposes a separate stable-cache contract.
        style: Option<CustomPaintStyle>,
        phase: RenderPhase,
    },
    Line {
        id: UiId,
        start: super::Point,
        end: super::Point,
        stroke: super::Stroke,
        phase: RenderPhase,
    },
    Path {
        id: UiId,
        rect: UiRect,
        path: UiPath,
        style: PathStyle,
        phase: RenderPhase,
    },
    Image {
        id: UiId,
        rect: UiRect,
        source: UiImageSource,
        request: ImageRequest,
        fit: ImageFit,
        phase: RenderPhase,
    },
    Icon {
        id: UiId,
        rect: UiRect,
        key: &'static str,
        style: super::IconStyle,
        phase: RenderPhase,
    },
    Glow {
        id: UiId,
        rect: UiRect,
        color: super::Color,
        alpha: u8,
        phase: RenderPhase,
    },
    BackdropBlur {
        id: UiId,
        rect: UiRect,
        style: BackdropBlurStyle,
        phase: RenderPhase,
    },
    BackdropBlurPath {
        id: UiId,
        rect: UiRect,
        path: UiPath,
        style: BackdropBlurStyle,
        phase: RenderPhase,
    },
    Overlay {
        id: UiId,
        rect: UiRect,
        style: OverlayStyle,
        phase: RenderPhase,
    },
    CompositingLayer {
        id: UiId,
        rect: UiRect,
        spec: CompositingLayerSpec,
        /// Commands use layer-local coordinates so moving a layer does not invalidate its surface.
        commands: Vec<ScenePrimitive>,
        content_signature: u64,
        phase: RenderPhase,
    },
    StaticLayer {
        id: UiId,
        rect: UiRect,
        spec: StaticLayerSpec,
        commands: Vec<ScenePrimitive>,
        child_signature: u64,
        phase: RenderPhase,
    },
    ScrollRaster {
        id: UiId,
        viewport: UiRect,
        spec: ScrollRasterSpec,
        commands: Vec<ScenePrimitive>,
        child_signature: u64,
        phase: RenderPhase,
    },
    Clip {
        id: UiId,
        rect: UiRect,
        commands: Vec<ScenePrimitive>,
        child_signature: u64,
        phase: RenderPhase,
    },
    ClipPath {
        id: UiId,
        rect: UiRect,
        path: UiPath,
        commands: Vec<ScenePrimitive>,
        child_signature: u64,
        phase: RenderPhase,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScenePrimitiveKind {
    Rect,
    Ellipse,
    Text,
    Custom,
    Line,
    Path,
    Image,
    Icon,
    Glow,
    BackdropBlur,
    BackdropBlurPath,
    Overlay,
    CompositingLayer,
    StaticLayer,
    ScrollRaster,
    Clip,
    ClipPath,
}

impl ScenePrimitiveKind {
    pub const ALL: [Self; 17] = [
        Self::Rect,
        Self::Ellipse,
        Self::Text,
        Self::Custom,
        Self::Line,
        Self::Path,
        Self::Image,
        Self::Icon,
        Self::Glow,
        Self::BackdropBlur,
        Self::BackdropBlurPath,
        Self::Overlay,
        Self::CompositingLayer,
        Self::StaticLayer,
        Self::ScrollRaster,
        Self::Clip,
        Self::ClipPath,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "Rect",
            Self::Ellipse => "Ellipse",
            Self::Text => "Text",
            Self::Custom => "Custom",
            Self::Line => "Line",
            Self::Path => "Path",
            Self::Image => "Image",
            Self::Icon => "Icon",
            Self::Glow => "Glow",
            Self::BackdropBlur => "BackdropBlur",
            Self::BackdropBlurPath => "BackdropBlurPath",
            Self::Overlay => "Overlay",
            Self::CompositingLayer => "CompositingLayer",
            Self::StaticLayer => "StaticLayer",
            Self::ScrollRaster => "ScrollRaster",
            Self::Clip => "Clip",
            Self::ClipPath => "ClipPath",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScrollRasterSpec {
    pub cache_epoch: u64,
    pub content_height: f32,
    pub scroll_y: f32,
    pub tile_height_px: f32,
    pub memory_budget_bytes: usize,
    pub background_fill: Option<Color>,
    pub visible_tiles: Vec<usize>,
    pub prefetch_tiles: Vec<usize>,
    pub max_prefetch_tiles_per_frame: usize,
    pub max_prefetch_ms_per_frame: u32,
}

impl Eq for ScrollRasterSpec {}

impl Hash for ScrollRasterSpec {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cache_epoch.hash(state);
        normalized_f32_bits(self.content_height).hash(state);
        normalized_f32_bits(self.scroll_y).hash(state);
        normalized_f32_bits(self.tile_height_px).hash(state);
        self.memory_budget_bytes.hash(state);
        self.background_fill.hash(state);
        self.visible_tiles.hash(state);
        self.prefetch_tiles.hash(state);
        self.max_prefetch_tiles_per_frame.hash(state);
        self.max_prefetch_ms_per_frame.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

impl ScenePrimitive {
    pub(crate) fn contains_shadow(&self) -> bool {
        match self {
            Self::CompositingLayer { spec, commands, .. } => {
                spec.shadow.is_some() || commands.iter().any(Self::contains_shadow)
            }
            Self::StaticLayer { commands, .. }
            | Self::ScrollRaster { commands, .. }
            | Self::Clip { commands, .. }
            | Self::ClipPath { commands, .. } => commands.iter().any(Self::contains_shadow),
            _ => false,
        }
    }

    pub const fn kind(&self) -> ScenePrimitiveKind {
        match self {
            Self::Rect { .. } => ScenePrimitiveKind::Rect,
            Self::Ellipse { .. } => ScenePrimitiveKind::Ellipse,
            Self::Text { .. } => ScenePrimitiveKind::Text,
            Self::Custom { .. } => ScenePrimitiveKind::Custom,
            Self::Line { .. } => ScenePrimitiveKind::Line,
            Self::Path { .. } => ScenePrimitiveKind::Path,
            Self::Image { .. } => ScenePrimitiveKind::Image,
            Self::Icon { .. } => ScenePrimitiveKind::Icon,
            Self::Glow { .. } => ScenePrimitiveKind::Glow,
            Self::BackdropBlur { .. } => ScenePrimitiveKind::BackdropBlur,
            Self::BackdropBlurPath { .. } => ScenePrimitiveKind::BackdropBlurPath,
            Self::Overlay { .. } => ScenePrimitiveKind::Overlay,
            Self::CompositingLayer { .. } => ScenePrimitiveKind::CompositingLayer,
            Self::StaticLayer { .. } => ScenePrimitiveKind::StaticLayer,
            Self::ScrollRaster { .. } => ScenePrimitiveKind::ScrollRaster,
            Self::Clip { .. } => ScenePrimitiveKind::Clip,
            Self::ClipPath { .. } => ScenePrimitiveKind::ClipPath,
        }
    }

    pub fn id(&self) -> &UiId {
        match self {
            ScenePrimitive::Rect { id, .. }
            | ScenePrimitive::Ellipse { id, .. }
            | ScenePrimitive::Text { id, .. }
            | ScenePrimitive::Custom { id, .. }
            | ScenePrimitive::Line { id, .. }
            | ScenePrimitive::Path { id, .. }
            | ScenePrimitive::Image { id, .. }
            | ScenePrimitive::Icon { id, .. }
            | ScenePrimitive::Glow { id, .. }
            | ScenePrimitive::BackdropBlur { id, .. }
            | ScenePrimitive::BackdropBlurPath { id, .. }
            | ScenePrimitive::Overlay { id, .. }
            | ScenePrimitive::CompositingLayer { id, .. }
            | ScenePrimitive::StaticLayer { id, .. }
            | ScenePrimitive::ScrollRaster { id, .. }
            | ScenePrimitive::Clip { id, .. }
            | ScenePrimitive::ClipPath { id, .. } => id,
        }
    }

    pub fn phase(&self) -> RenderPhase {
        match self {
            ScenePrimitive::Rect { phase, .. }
            | ScenePrimitive::Ellipse { phase, .. }
            | ScenePrimitive::Text { phase, .. }
            | ScenePrimitive::Custom { phase, .. }
            | ScenePrimitive::Line { phase, .. }
            | ScenePrimitive::Path { phase, .. }
            | ScenePrimitive::Image { phase, .. }
            | ScenePrimitive::Icon { phase, .. }
            | ScenePrimitive::Glow { phase, .. }
            | ScenePrimitive::BackdropBlur { phase, .. }
            | ScenePrimitive::BackdropBlurPath { phase, .. }
            | ScenePrimitive::Overlay { phase, .. }
            | ScenePrimitive::CompositingLayer { phase, .. }
            | ScenePrimitive::StaticLayer { phase, .. }
            | ScenePrimitive::ScrollRaster { phase, .. }
            | ScenePrimitive::Clip { phase, .. }
            | ScenePrimitive::ClipPath { phase, .. } => *phase,
        }
    }

    pub fn rect(&self) -> UiRect {
        match self {
            ScenePrimitive::Rect { rect, .. }
            | ScenePrimitive::Ellipse { rect, .. }
            | ScenePrimitive::Text { rect, .. }
            | ScenePrimitive::Custom { rect, .. }
            | ScenePrimitive::Image { rect, .. }
            | ScenePrimitive::Path { rect, .. }
            | ScenePrimitive::Icon { rect, .. }
            | ScenePrimitive::Glow { rect, .. }
            | ScenePrimitive::BackdropBlur { rect, .. }
            | ScenePrimitive::BackdropBlurPath { rect, .. }
            | ScenePrimitive::Overlay { rect, .. } => *rect,
            ScenePrimitive::CompositingLayer { rect, .. } => *rect,
            ScenePrimitive::StaticLayer { rect, spec, .. } => {
                rect.translate(spec.offset_x, spec.offset_y)
            }
            ScenePrimitive::ScrollRaster { viewport, .. } => *viewport,
            ScenePrimitive::Clip { rect, .. } | ScenePrimitive::ClipPath { rect, .. } => *rect,
            ScenePrimitive::Line { start, end, .. } => UiRect::new(
                start.x.min(end.x),
                start.y.min(end.y),
                start.x.max(end.x) + 1.0,
                start.y.max(end.y) + 1.0,
            ),
        }
    }

    pub fn paint_bounds(&self) -> UiRect {
        let rect = match self {
            ScenePrimitive::CompositingLayer { rect, spec, .. } => {
                spec.transform.transformed_bounds(*rect)
            }
            _ => self.rect(),
        };
        let stroke_width = match self {
            ScenePrimitive::Rect { style, .. } | ScenePrimitive::Ellipse { style, .. } => {
                style.stroke.map(|stroke| stroke.width).unwrap_or(0.0)
            }
            ScenePrimitive::Line { stroke, .. } => stroke.width,
            ScenePrimitive::Path { style, .. } => {
                style.stroke.map(|stroke| stroke.width).unwrap_or(0.0)
            }
            _ => 0.0,
        };
        let outset = (stroke_width.max(0.0) + 1.0) / 2.0;
        rect.inflate(outset, outset)
    }

    pub fn signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        command_signature_part(self, &mut hasher);
        hasher.finish()
    }
}
