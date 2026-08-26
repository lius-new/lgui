use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use super::{
    BackdropBlurStyle, Color, CustomPaintStyle, HostTree, OverlayStyle, PathStyle, StaticLayerSpec,
    TextStyle, UiId, UiImageSource, UiNode, UiNodeKind, UiPath, UiPathCommand, UiRect, UiScale,
    VisualStyle,
};

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScrollRasterSpec {
    pub cache_epoch: u64,
    pub content_height: i32,
    pub scroll_y: i32,
    pub tile_height_px: i32,
    pub memory_budget_bytes: usize,
    pub background_fill: Option<Color>,
    pub visible_tiles: Vec<usize>,
    pub prefetch_tiles: Vec<usize>,
    pub max_prefetch_tiles_per_frame: usize,
    pub max_prefetch_ms_per_frame: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
}

impl ScenePrimitive {
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
            ScenePrimitive::StaticLayer { rect, spec, .. } => {
                rect.translate(spec.offset_x, spec.offset_y)
            }
            ScenePrimitive::ScrollRaster { viewport, .. } => *viewport,
            ScenePrimitive::Clip { rect, .. } | ScenePrimitive::ClipPath { rect, .. } => *rect,
            ScenePrimitive::Line { start, end, .. } => UiRect::new(
                start.x.min(end.x),
                start.y.min(end.y),
                start.x.max(end.x) + 1,
                start.y.max(end.y) + 1,
            ),
        }
    }

    pub fn signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        command_signature_part(self, &mut hasher);
        hasher.finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    commands: Arc<Vec<ScenePrimitive>>,
}

const SCROLL_RASTER_COMMAND_CACHE_LIMIT: usize = 8;

#[derive(Clone, Hash, PartialEq, Eq)]
struct ScrollRasterCommandCacheKey {
    id: String,
    cache_epoch: u64,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    content_height: i32,
}

#[derive(Clone)]
struct ScrollRasterCommandSnapshot {
    commands: Vec<ScenePrimitive>,
    child_signature: u64,
    last_used: u64,
}

#[derive(Default)]
struct ScrollRasterCommandCache {
    entries: HashMap<ScrollRasterCommandCacheKey, ScrollRasterCommandSnapshot>,
    tick: u64,
}

fn scroll_raster_command_cache() -> &'static Mutex<ScrollRasterCommandCache> {
    static CACHE: OnceLock<Mutex<ScrollRasterCommandCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ScrollRasterCommandCache::default()))
}

impl Scene {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(Vec::new()),
        }
    }

    pub fn push(&mut self, command: ScenePrimitive) {
        Arc::make_mut(&mut self.commands).push(command);
    }

    pub fn commands(&self) -> &[ScenePrimitive] {
        self.commands.as_slice()
    }

    fn move_popup_commands_to_end(&mut self) {
        let commands = Arc::make_mut(&mut self.commands);
        let (popup, regular): (Vec<_>, Vec<_>) = std::mem::take(commands)
            .into_iter()
            .partition(|command| command.phase() == RenderPhase::Popup);
        *commands = regular;
        commands.extend(popup);
    }

    pub(crate) fn replace_range(
        &mut self,
        range: std::ops::Range<usize>,
        commands: impl IntoIterator<Item = ScenePrimitive>,
    ) {
        Arc::make_mut(&mut self.commands).splice(range, commands);
    }

    pub(crate) fn replace_all(&mut self, commands: Vec<ScenePrimitive>) {
        self.commands = Arc::new(commands);
    }

    pub fn bounds(&self) -> Option<UiRect> {
        self.commands()
            .iter()
            .map(ScenePrimitive::rect)
            .reduce(UiRect::union)
    }

    pub fn project_to_physical(&self, scale: UiScale) -> Self {
        if scale.is_identity() {
            return self.clone();
        }
        Self {
            commands: Arc::new(
                self.commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
            ),
        }
    }
}

fn project_command(command: &ScenePrimitive, scale: UiScale) -> ScenePrimitive {
    let signature_scale = scale.factor().to_bits() as u64;
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Rect {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            style: project_visual_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Ellipse {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            style: project_visual_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => ScenePrimitive::Text {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            text: text.clone(),
            style: project_text_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Custom {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => ScenePrimitive::Line {
            id: id.clone(),
            start: scale.physical_point(*start),
            end: scale.physical_point(*end),
            stroke: project_stroke(*stroke, scale),
            phase: *phase,
        },
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::Path {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            path: project_path(path, scale),
            style: project_path_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Image {
            id,
            rect,
            source,
            fit,
            phase,
        } => ScenePrimitive::Image {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            source: source.clone(),
            fit: *fit,
            phase: *phase,
        },
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Icon {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => ScenePrimitive::Glow {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            color: *color,
            alpha: *alpha,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::BackdropBlur {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            style: project_backdrop_blur_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::BackdropBlurPath {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            path: project_path(path, scale),
            style: project_backdrop_blur_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Overlay {
            id: id.clone(),
            rect: scale.physical_rect(*rect),
            style: style.clone(),
            phase: *phase,
        },
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => {
            let mut spec = spec.clone();
            spec.offset_x = scale.physical_value(spec.offset_x);
            spec.offset_y = scale.physical_value(spec.offset_y);
            ScenePrimitive::StaticLayer {
                id: id.clone(),
                rect: scale.physical_rect(*rect),
                spec,
                commands: commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
                child_signature: child_signature ^ signature_scale,
                phase: *phase,
            }
        }
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            commands,
            child_signature,
            phase,
        } => {
            let mut spec = spec.clone();
            spec.cache_epoch ^= signature_scale;
            spec.content_height = scale.physical_length(spec.content_height);
            spec.scroll_y = scale.physical_value(spec.scroll_y);
            spec.tile_height_px = scale.physical_length(spec.tile_height_px);
            ScenePrimitive::ScrollRaster {
                id: id.clone(),
                viewport: scale.physical_rect(*viewport),
                spec,
                commands: commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
                child_signature: child_signature ^ signature_scale,
                phase: *phase,
            }
        }
        ScenePrimitive::Clip {
            id,
            rect,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::Clip {
            id: id.clone(),
            rect: scale.physical_rect_outward(*rect),
            commands: commands
                .iter()
                .map(|command| project_command(command, scale))
                .collect(),
            child_signature: child_signature ^ signature_scale,
            phase: *phase,
        },
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ClipPath {
            id: id.clone(),
            rect: scale.physical_rect_outward(*rect),
            path: project_path(path, scale),
            commands: commands
                .iter()
                .map(|command| project_command(command, scale))
                .collect(),
            child_signature: child_signature ^ signature_scale,
            phase: *phase,
        },
    }
}

fn project_stroke(mut stroke: super::Stroke, scale: UiScale) -> super::Stroke {
    stroke.width = scale.physical_length(stroke.width);
    stroke
}

fn project_visual_style(mut style: VisualStyle, scale: UiScale) -> VisualStyle {
    style.radius = scale.physical_length(style.radius);
    style.stroke = style.stroke.map(|stroke| project_stroke(stroke, scale));
    style
}

fn project_path_style(mut style: PathStyle, scale: UiScale) -> PathStyle {
    style.stroke = style.stroke.map(|stroke| project_stroke(stroke, scale));
    style
}

fn project_text_style(mut style: TextStyle, scale: UiScale) -> TextStyle {
    style.height = scale.physical_signed_length(style.height);
    style.tracking = scale.physical_value(style.tracking);
    style
}

fn project_backdrop_blur_style(mut style: BackdropBlurStyle, scale: UiScale) -> BackdropBlurStyle {
    style.source_rect = scale.physical_rect(style.source_rect);
    style.radius = scale.physical_length(style.radius as i32) as usize;
    style
}

fn project_path(path: &UiPath, scale: UiScale) -> UiPath {
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(scale.physical_point(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(scale.physical_point(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: scale.physical_point(control),
            to: scale.physical_point(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: scale.physical_point(control1),
            control2: scale.physical_point(control2),
            to: scale.physical_point(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}

pub fn compile_scene(tree: &HostTree) -> Scene {
    let mut list = Scene::new();
    let mut skip = HashSet::new();
    for node in tree.nodes() {
        if skip.contains(&node.id) {
            continue;
        }
        let command_start = list.commands.len();
        let include_popup_subtree = node.render_phase == RenderPhase::Popup;
        if matches!(
            node.kind,
            UiNodeKind::StaticLayer
                | UiNodeKind::ScrollRaster
                | UiNodeKind::Clip
                | UiNodeKind::ClipPath
        ) {
            if let Some(spec) = node.static_layer.clone() {
                let commands =
                    compile_static_layer_commands(tree, node, &mut skip, include_popup_subtree);
                let child_signature = command_signature(&commands);
                list.push(ScenePrimitive::StaticLayer {
                    id: node.id.clone(),
                    rect: node.layout_rect,
                    spec,
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
            if let Some(spec) = node.scroll_raster.clone() {
                let (commands, child_signature) = compile_scroll_raster_commands(
                    tree,
                    node,
                    &spec,
                    &mut skip,
                    include_popup_subtree,
                );
                list.push(ScenePrimitive::ScrollRaster {
                    id: node.id.clone(),
                    viewport: node.layout_rect,
                    spec,
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
            if node.kind == UiNodeKind::Clip {
                let commands = compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
                let child_signature = command_signature(&commands);
                list.push(ScenePrimitive::Clip {
                    id: node.id.clone(),
                    rect: node.clip_rect.unwrap_or(node.layout_rect),
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
            if node.kind == UiNodeKind::ClipPath {
                if let Some(path) = node.path.clone() {
                    let commands =
                        compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
                    let child_signature = command_signature(&commands);
                    list.push(ScenePrimitive::ClipPath {
                        id: node.id.clone(),
                        rect: node.clip_rect.unwrap_or(node.layout_rect),
                        path,
                        commands,
                        child_signature,
                        phase: node.render_phase,
                    });
                }
            }
            translate_popup_root_commands(&mut list, tree, node, command_start);
            continue;
        }
        if matches!(node.kind, UiNodeKind::Group | UiNodeKind::Root) {
            continue;
        }
        push_node_commands(&mut list, node);
        translate_popup_root_commands(&mut list, tree, node, command_start);
    }
    list.move_popup_commands_to_end();
    list
}

/// Returns the retained scene roots in backend paint order without compiling their primitives.
/// Descendants owned by clip/static/raster containers are represented by that container root;
/// popup descendants remain independent roots so they preserve popup z-order semantics.
pub fn scene_root_ids(tree: &HostTree) -> Vec<UiId> {
    let mut roots = Vec::new();
    let mut skip = HashSet::new();
    for node in tree.nodes() {
        if skip.contains(&node.id) {
            continue;
        }
        if matches!(
            node.kind,
            UiNodeKind::StaticLayer
                | UiNodeKind::ScrollRaster
                | UiNodeKind::Clip
                | UiNodeKind::ClipPath
        ) {
            roots.push(node.id.clone());
            mark_node_and_descendants_skipped(
                tree,
                node,
                &mut skip,
                node.render_phase == RenderPhase::Popup,
            );
        } else if !matches!(node.kind, UiNodeKind::Group | UiNodeKind::Root) {
            roots.push(node.id.clone());
        }
    }
    roots.sort_by_key(|id| {
        tree.node(id)
            .is_some_and(|node| node.render_phase == RenderPhase::Popup)
    });
    roots
}

/// Compiles one retained scene root. Normal commits call this only for inserted or paint-dirty
/// roots; unchanged roots keep their previously committed primitive vectors.
pub fn compile_scene_root(tree: &HostTree, id: &UiId) -> Scene {
    let Some(node) = tree.node(id) else {
        return Scene::new();
    };
    let mut list = Scene::new();
    let mut skip = HashSet::new();
    let include_popup_subtree = node.render_phase == RenderPhase::Popup;
    if matches!(
        node.kind,
        UiNodeKind::StaticLayer
            | UiNodeKind::ScrollRaster
            | UiNodeKind::Clip
            | UiNodeKind::ClipPath
    ) {
        if let Some(spec) = node.static_layer.clone() {
            let commands =
                compile_static_layer_commands(tree, node, &mut skip, include_popup_subtree);
            let child_signature = command_signature(&commands);
            list.push(ScenePrimitive::StaticLayer {
                id: node.id.clone(),
                rect: node.layout_rect,
                spec,
                commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        if let Some(spec) = node.scroll_raster.clone() {
            let (commands, child_signature) =
                compile_scroll_raster_commands(tree, node, &spec, &mut skip, include_popup_subtree);
            list.push(ScenePrimitive::ScrollRaster {
                id: node.id.clone(),
                viewport: node.layout_rect,
                spec,
                commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        if node.kind == UiNodeKind::Clip {
            let commands = compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
            let child_signature = command_signature(&commands);
            list.push(ScenePrimitive::Clip {
                id: node.id.clone(),
                rect: node.clip_rect.unwrap_or(node.layout_rect),
                commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        if node.kind == UiNodeKind::ClipPath {
            if let Some(path) = node.path.clone() {
                let commands = compile_clip_commands(tree, node, &mut skip, include_popup_subtree);
                let child_signature = command_signature(&commands);
                list.push(ScenePrimitive::ClipPath {
                    id: node.id.clone(),
                    rect: node.clip_rect.unwrap_or(node.layout_rect),
                    path,
                    commands,
                    child_signature,
                    phase: node.render_phase,
                });
            }
        }
    } else if !matches!(node.kind, UiNodeKind::Group | UiNodeKind::Root) {
        push_node_commands(&mut list, node);
    }
    translate_popup_root_commands(&mut list, tree, node, 0);
    list
}

fn translate_popup_root_commands(
    list: &mut Scene,
    tree: &HostTree,
    node: &UiNode,
    command_start: usize,
) {
    if node.render_phase != RenderPhase::Popup {
        return;
    }
    let (offset_x, offset_y) = tree.ancestor_content_offset(node);
    if offset_x == 0 && offset_y == 0 {
        return;
    }
    for command in &mut Arc::make_mut(&mut list.commands)[command_start..] {
        *command = translate_command(command, offset_x, offset_y);
    }
}

fn compile_static_layer_commands(
    tree: &HostTree,
    root: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> Vec<ScenePrimitive> {
    let mut commands = Vec::new();
    for child_id in &root.children {
        compile_static_layer_child(tree, child_id, &mut commands, skip, include_popup_subtree);
    }
    commands
}

fn compile_scroll_raster_commands(
    tree: &HostTree,
    root: &UiNode,
    spec: &ScrollRasterSpec,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> (Vec<ScenePrimitive>, u64) {
    if let Some(snapshot) = load_scroll_raster_command_snapshot(root, spec) {
        mark_node_and_descendants_skipped(tree, root, skip, include_popup_subtree);
        return snapshot;
    }
    let commands = compile_static_layer_commands(tree, root, skip, include_popup_subtree);
    let child_signature = command_signature(&commands);
    store_scroll_raster_command_snapshot(root, spec, commands.clone(), child_signature);
    (commands, child_signature)
}

fn mark_node_and_descendants_skipped(
    tree: &HostTree,
    node: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) {
    skip.insert(node.id.clone());
    for child_id in &node.children {
        if let Some(child) = tree.node(child_id) {
            if child.render_phase == RenderPhase::Popup && !include_popup_subtree {
                continue;
            }
            mark_node_and_descendants_skipped(tree, child, skip, include_popup_subtree);
        }
    }
}

fn load_scroll_raster_command_snapshot(
    node: &UiNode,
    spec: &ScrollRasterSpec,
) -> Option<(Vec<ScenePrimitive>, u64)> {
    let key = scroll_raster_command_cache_key(node, spec);
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    cache.tick = cache.tick.saturating_add(1);
    let tick = cache.tick;
    let snapshot = cache.entries.get_mut(&key)?;
    snapshot.last_used = tick;
    Some((snapshot.commands.clone(), snapshot.child_signature))
}

pub fn scroll_raster_command_snapshot_exists(
    id: &UiId,
    rect: UiRect,
    spec: &ScrollRasterSpec,
) -> bool {
    let key = scroll_raster_command_cache_key_for(id, rect, spec);
    scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned")
        .entries
        .contains_key(&key)
}

fn store_scroll_raster_command_snapshot(
    node: &UiNode,
    spec: &ScrollRasterSpec,
    commands: Vec<ScenePrimitive>,
    child_signature: u64,
) {
    let key = scroll_raster_command_cache_key(node, spec);
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    cache.tick = cache.tick.saturating_add(1);
    let tick = cache.tick;
    cache.entries.insert(
        key,
        ScrollRasterCommandSnapshot {
            commands,
            child_signature,
            last_used: tick,
        },
    );
    while cache.entries.len() > SCROLL_RASTER_COMMAND_CACHE_LIMIT {
        let Some(oldest_key) = cache
            .entries
            .iter()
            .min_by_key(|(_, snapshot)| snapshot.last_used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        cache.entries.remove(&oldest_key);
    }
}

fn scroll_raster_command_cache_key(
    node: &UiNode,
    spec: &ScrollRasterSpec,
) -> ScrollRasterCommandCacheKey {
    scroll_raster_command_cache_key_for(&node.id, node.layout_rect, spec)
}

fn scroll_raster_command_cache_key_for(
    id: &UiId,
    rect: UiRect,
    spec: &ScrollRasterSpec,
) -> ScrollRasterCommandCacheKey {
    ScrollRasterCommandCacheKey {
        id: id.as_str().to_string(),
        cache_epoch: spec.cache_epoch,
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        content_height: spec.content_height,
    }
}

fn compile_static_layer_child(
    tree: &HostTree,
    id: &UiId,
    commands: &mut Vec<ScenePrimitive>,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) {
    let Some(node) = tree.node(id) else {
        return;
    };
    if node.render_phase == RenderPhase::Popup && !include_popup_subtree {
        return;
    }
    skip.insert(node.id.clone());
    push_node_and_children(tree, node, commands, skip, include_popup_subtree);
}

fn compile_clip_commands(
    tree: &HostTree,
    root: &UiNode,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) -> Vec<ScenePrimitive> {
    let mut commands = Vec::new();
    let (offset_x, offset_y) = root.content_offset;
    for child_id in &root.children {
        compile_clip_child(
            tree,
            child_id,
            &mut commands,
            skip,
            offset_x,
            offset_y,
            include_popup_subtree,
        );
    }
    commands
}

fn compile_clip_child(
    tree: &HostTree,
    id: &UiId,
    commands: &mut Vec<ScenePrimitive>,
    skip: &mut HashSet<UiId>,
    offset_x: i32,
    offset_y: i32,
    include_popup_subtree: bool,
) {
    let Some(node) = tree.node(id) else {
        return;
    };
    if node.render_phase == RenderPhase::Popup && !include_popup_subtree {
        return;
    }
    let before = commands.len();
    push_node_and_children(tree, node, commands, skip, include_popup_subtree);
    for command in &mut commands[before..] {
        *command = translate_command(command, offset_x, offset_y);
    }
}

fn push_node_and_children(
    tree: &HostTree,
    node: &UiNode,
    commands: &mut Vec<ScenePrimitive>,
    skip: &mut HashSet<UiId>,
    include_popup_subtree: bool,
) {
    if node.render_phase == RenderPhase::Popup && !include_popup_subtree {
        return;
    }
    skip.insert(node.id.clone());
    if node.kind == UiNodeKind::Clip {
        let nested = compile_clip_commands(tree, node, skip, include_popup_subtree);
        let child_signature = command_signature(&nested);
        commands.push(ScenePrimitive::Clip {
            id: node.id.clone(),
            rect: node.clip_rect.unwrap_or(node.layout_rect),
            commands: nested,
            child_signature,
            phase: node.render_phase,
        });
        return;
    }
    if node.kind == UiNodeKind::ClipPath {
        if let Some(path) = node.path.clone() {
            let nested = compile_clip_commands(tree, node, skip, include_popup_subtree);
            let child_signature = command_signature(&nested);
            commands.push(ScenePrimitive::ClipPath {
                id: node.id.clone(),
                rect: node.clip_rect.unwrap_or(node.layout_rect),
                path,
                commands: nested,
                child_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if let UiNodeKind::StaticLayer = node.kind {
        if let Some(spec) = node.static_layer.clone() {
            let layer_commands =
                compile_static_layer_commands(tree, node, skip, include_popup_subtree);
            let child_signature = command_signature(&layer_commands);
            commands.push(ScenePrimitive::StaticLayer {
                id: node.id.clone(),
                rect: node.layout_rect,
                spec,
                commands: layer_commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if let UiNodeKind::ScrollRaster = node.kind {
        if let Some(spec) = node.scroll_raster.clone() {
            let (raster_commands, child_signature) =
                compile_scroll_raster_commands(tree, node, &spec, skip, include_popup_subtree);
            commands.push(ScenePrimitive::ScrollRaster {
                id: node.id.clone(),
                viewport: node.layout_rect,
                spec,
                commands: raster_commands,
                child_signature,
                phase: node.render_phase,
            });
        }
        return;
    }
    if !matches!(
        node.kind,
        UiNodeKind::Group
            | UiNodeKind::Root
            | UiNodeKind::StaticLayer
            | UiNodeKind::ScrollRaster
            | UiNodeKind::ClipPath
    ) {
        push_node_commands_vec(commands, node);
    }
    for child_id in &node.children {
        let Some(child) = tree.node(child_id) else {
            continue;
        };
        if child.render_phase == RenderPhase::Popup && !include_popup_subtree {
            continue;
        }
        push_node_and_children(tree, child, commands, skip, include_popup_subtree);
    }
}

fn push_node_commands(list: &mut Scene, node: &UiNode) {
    push_node_commands_into(|command| list.push(command), node);
}

fn push_node_commands_vec(commands: &mut Vec<ScenePrimitive>, node: &UiNode) {
    push_node_commands_into(|command| commands.push(command), node);
}

fn push_node_commands_into(mut push: impl FnMut(ScenePrimitive), node: &UiNode) {
    if !matches!(node.kind, UiNodeKind::Ellipse)
        && (node.style.fill.is_some() || node.style.stroke.is_some())
    {
        push(ScenePrimitive::Rect {
            id: node.id.clone(),
            rect: node.layout_rect,
            style: node.style,
            phase: node.render_phase,
        });
    }
    if let UiNodeKind::Ellipse = node.kind {
        if node.style.fill.is_some() || node.style.stroke.is_some() {
            push(ScenePrimitive::Ellipse {
                id: node.id.clone(),
                rect: node.layout_rect,
                style: node.style,
                phase: node.render_phase,
            });
        }
    }
    if let (Some(text), Some(style)) = (node.text.clone(), node.text_style) {
        push(ScenePrimitive::Text {
            id: node.id.clone(),
            rect: node.layout_rect,
            text,
            style,
            phase: node.render_phase,
        });
    }
    if let UiNodeKind::Custom(key) = node.kind {
        push(ScenePrimitive::Custom {
            id: node.id.clone(),
            rect: node.layout_rect,
            key,
            style: node.custom_style,
            phase: node.render_phase,
        });
    }
    if let UiNodeKind::Line = node.kind {
        if let Some(stroke) = node.style.stroke {
            push(ScenePrimitive::Line {
                id: node.id.clone(),
                start: super::Point::new(node.layout_rect.left, node.layout_rect.top),
                end: super::Point::new(node.layout_rect.right, node.layout_rect.bottom),
                stroke,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Path = node.kind {
        if let Some(path) = node.path.clone() {
            push(ScenePrimitive::Path {
                id: node.id.clone(),
                rect: node.layout_rect,
                path,
                style: node.path_style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Image = node.kind {
        if let Some(source) = node.image_source.as_ref() {
            push(ScenePrimitive::Image {
                id: node.id.clone(),
                rect: node.layout_rect,
                source: source.clone(),
                fit: node.image_fit,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Icon = node.kind {
        if let Some(key) = node.icon_key {
            push(ScenePrimitive::Icon {
                id: node.id.clone(),
                rect: node.layout_rect,
                key,
                style: node.icon_style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Glow = node.kind {
        if let Some((color, alpha)) = node.glow {
            push(ScenePrimitive::Glow {
                id: node.id.clone(),
                rect: node.layout_rect,
                color,
                alpha,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::BackdropBlur = node.kind {
        if let Some(style) = node.backdrop_blur_style {
            push(ScenePrimitive::BackdropBlur {
                id: node.id.clone(),
                rect: node.layout_rect,
                style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::BackdropBlurPath = node.kind {
        if let (Some(style), Some(path)) = (node.backdrop_blur_style, node.path.clone()) {
            push(ScenePrimitive::BackdropBlurPath {
                id: node.id.clone(),
                rect: node.layout_rect,
                path,
                style,
                phase: node.render_phase,
            });
        }
    }
    if let UiNodeKind::Overlay = node.kind {
        if let Some(style) = node.overlay_style.clone() {
            push(ScenePrimitive::Overlay {
                id: node.id.clone(),
                rect: node.layout_rect,
                style,
                phase: node.render_phase,
            });
        }
    }
}

fn command_signature(commands: &[ScenePrimitive]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for command in commands {
        command_signature_part(command, &mut hasher);
    }
    hasher.finish()
}

fn command_signature_part(command: &ScenePrimitive, hasher: &mut DefaultHasher) {
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => {
            "rect".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_visual_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => {
            "ellipse".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_visual_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => {
            "text".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            text.hash(hasher);
            hash_text_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => {
            "custom".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            key.hash(hasher);
            hash_custom_paint_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => {
            "line".hash(hasher);
            id.hash(hasher);
            hash_point(start, hasher);
            hash_point(end, hasher);
            hash_stroke(stroke, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => {
            "path".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_path(path, hasher);
            hash_path_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Image {
            id,
            rect,
            source,
            fit,
            phase,
        } => {
            "image".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            source.hash(hasher);
            fit.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => {
            "icon".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            key.hash(hasher);
            hash_icon_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => {
            "glow".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_color(color, hasher);
            alpha.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => {
            "backdrop-blur".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_backdrop_blur_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => {
            "backdrop-blur-path".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_path(path, hasher);
            hash_backdrop_blur_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => {
            "overlay".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_overlay_style(style, hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            child_signature,
            phase,
            ..
        } => {
            "static-layer".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            spec.cache_signature().hash(hasher);
            spec.opacity.hash(hasher);
            spec.offset_x.hash(hasher);
            spec.offset_y.hash(hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            child_signature,
            phase,
            ..
        } => {
            "scroll-raster".hash(hasher);
            id.hash(hasher);
            hash_rect(viewport, hasher);
            spec.hash(hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::Clip {
            id,
            rect,
            child_signature,
            phase,
            ..
        } => {
            "clip".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            child_signature,
            phase,
            ..
        } => {
            "clip-path".hash(hasher);
            id.hash(hasher);
            hash_rect(rect, hasher);
            hash_path(path, hasher);
            child_signature.hash(hasher);
            phase.hash(hasher);
        }
    }
}

fn translate_commands(commands: Vec<ScenePrimitive>, dx: i32, dy: i32) -> Vec<ScenePrimitive> {
    commands
        .into_iter()
        .map(|command| translate_command(&command, dx, dy))
        .collect()
}

fn translate_command(command: &ScenePrimitive, dx: i32, dy: i32) -> ScenePrimitive {
    let translate_rect = |rect: UiRect| rect.translate(dx, dy);
    let translate_point = |point: super::Point| super::Point::new(point.x + dx, point.y + dy);
    match command {
        ScenePrimitive::Rect {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Rect {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Ellipse {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Ellipse {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Text {
            id,
            rect,
            text,
            style,
            phase,
        } => ScenePrimitive::Text {
            id: id.clone(),
            rect: translate_rect(*rect),
            text: text.clone(),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Custom {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Custom {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Line {
            id,
            start,
            end,
            stroke,
            phase,
        } => ScenePrimitive::Line {
            id: id.clone(),
            start: translate_point(*start),
            end: translate_point(*end),
            stroke: *stroke,
            phase: *phase,
        },
        ScenePrimitive::Path {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::Path {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Image {
            id,
            rect,
            source,
            fit,
            phase,
        } => ScenePrimitive::Image {
            id: id.clone(),
            rect: translate_rect(*rect),
            source: source.clone(),
            fit: *fit,
            phase: *phase,
        },
        ScenePrimitive::Icon {
            id,
            rect,
            key,
            style,
            phase,
        } => ScenePrimitive::Icon {
            id: id.clone(),
            rect: translate_rect(*rect),
            key,
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Glow {
            id,
            rect,
            color,
            alpha,
            phase,
        } => ScenePrimitive::Glow {
            id: id.clone(),
            rect: translate_rect(*rect),
            color: *color,
            alpha: *alpha,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlur {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::BackdropBlur {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::BackdropBlurPath {
            id,
            rect,
            path,
            style,
            phase,
        } => ScenePrimitive::BackdropBlurPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            style: *style,
            phase: *phase,
        },
        ScenePrimitive::Overlay {
            id,
            rect,
            style,
            phase,
        } => ScenePrimitive::Overlay {
            id: id.clone(),
            rect: translate_rect(*rect),
            style: style.clone(),
            phase: *phase,
        },
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::StaticLayer {
            id: id.clone(),
            rect: translate_rect(*rect),
            spec: spec.clone(),
            commands: translate_commands(commands.clone(), dx, dy),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ScrollRaster {
            id,
            viewport,
            spec,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ScrollRaster {
            id: id.clone(),
            viewport: translate_rect(*viewport),
            spec: spec.clone(),
            commands: translate_commands(commands.clone(), dx, dy),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::Clip {
            id,
            rect,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::Clip {
            id: id.clone(),
            rect: translate_rect(*rect),
            commands: translate_commands(commands.clone(), dx, dy),
            child_signature: *child_signature,
            phase: *phase,
        },
        ScenePrimitive::ClipPath {
            id,
            rect,
            path,
            commands,
            child_signature,
            phase,
        } => ScenePrimitive::ClipPath {
            id: id.clone(),
            rect: translate_rect(*rect),
            path: translate_path(path, dx, dy),
            commands: translate_commands(commands.clone(), dx, dy),
            child_signature: *child_signature,
            phase: *phase,
        },
    }
}

fn translate_path(path: &UiPath, dx: i32, dy: i32) -> UiPath {
    let translate = |point: super::Point| super::Point::new(point.x + dx, point.y + dy);
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(translate(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(translate(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: translate(control),
            to: translate(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: translate(control1),
            control2: translate(control2),
            to: translate(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}

fn hash_custom_paint_style(style: &Option<CustomPaintStyle>, hasher: &mut DefaultHasher) {
    style.is_some().hash(hasher);
    if let Some(style) = style {
        hash_color(&style.color, hasher);
        style.intensity.to_bits().hash(hasher);
    }
}

fn hash_backdrop_blur_style(style: &BackdropBlurStyle, hasher: &mut DefaultHasher) {
    style.source.hash(hasher);
    style.fit.hash(hasher);
    hash_rect(&style.source_rect, hasher);
    style.radius.hash(hasher);
    style.opacity.to_bits().hash(hasher);
    hash_color(&style.tint, hasher);
    style.tint_alpha.to_bits().hash(hasher);
}

fn hash_rect(rect: &UiRect, hasher: &mut DefaultHasher) {
    rect.left.hash(hasher);
    rect.top.hash(hasher);
    rect.right.hash(hasher);
    rect.bottom.hash(hasher);
}

fn hash_point(point: &super::Point, hasher: &mut DefaultHasher) {
    point.x.hash(hasher);
    point.y.hash(hasher);
}

fn hash_color(color: &super::Color, hasher: &mut DefaultHasher) {
    color.0.hash(hasher);
}

fn hash_stroke(stroke: &super::Stroke, hasher: &mut DefaultHasher) {
    hash_color(&stroke.color, hasher);
    stroke.width.hash(hasher);
    stroke.alpha.hash(hasher);
}

fn hash_path(path: &UiPath, hasher: &mut DefaultHasher) {
    path.commands.len().hash(hasher);
    for command in &path.commands {
        match command {
            UiPathCommand::MoveTo(point) => {
                "move".hash(hasher);
                hash_point(point, hasher);
            }
            UiPathCommand::LineTo(point) => {
                "line".hash(hasher);
                hash_point(point, hasher);
            }
            UiPathCommand::QuadraticTo { control, to } => {
                "quadratic".hash(hasher);
                hash_point(control, hasher);
                hash_point(to, hasher);
            }
            UiPathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                "cubic".hash(hasher);
                hash_point(control1, hasher);
                hash_point(control2, hasher);
                hash_point(to, hasher);
            }
            UiPathCommand::Close => {
                "close".hash(hasher);
            }
        }
    }
}

fn hash_path_style(style: &PathStyle, hasher: &mut DefaultHasher) {
    style.fill.map(|color| color.0).hash(hasher);
    style.fill_alpha.hash(hasher);
    style.stroke.is_some().hash(hasher);
    if let Some(stroke) = &style.stroke {
        hash_stroke(stroke, hasher);
    }
}

fn hash_visual_style(style: &VisualStyle, hasher: &mut DefaultHasher) {
    style.fill.map(|color| color.0).hash(hasher);
    style.fill_alpha.hash(hasher);
    style.stroke.is_some().hash(hasher);
    if let Some(stroke) = &style.stroke {
        hash_stroke(stroke, hasher);
    }
    style.radius.hash(hasher);
}

fn hash_text_style(style: &TextStyle, hasher: &mut DefaultHasher) {
    hash_color(&style.color, hasher);
    style.height.hash(hasher);
    style.weight.hash(hasher);
    style.tracking.hash(hasher);
    style.align.hash(hasher);
    style.alpha.hash(hasher);
}

fn hash_icon_style(style: &super::IconStyle, hasher: &mut DefaultHasher) {
    hash_color(&style.color, hasher);
    style.alpha.hash(hasher);
}

fn hash_overlay_style(style: &OverlayStyle, hasher: &mut DefaultHasher) {
    style.vertical_layers.len().hash(hasher);
    for layer in &style.vertical_layers {
        hash_color(&layer.color, hasher);
        layer.alpha_top.to_bits().hash(hasher);
        layer.alpha_bottom.to_bits().hash(hasher);
    }
    style.radial_layers.len().hash(hasher);
    for layer in &style.radial_layers {
        hash_color(&layer.color, hasher);
        layer.alpha.to_bits().hash(hasher);
        layer.center_x.to_bits().hash(hasher);
        layer.center_y.to_bits().hash(hasher);
        layer.radius.to_bits().hash(hasher);
    }
}

pub fn commands_for_phase(
    commands: &[ScenePrimitive],
    phase: RenderPhase,
) -> impl Iterator<Item = &ScenePrimitive> {
    commands.iter().filter(move |command| match command {
        ScenePrimitive::Rect {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Ellipse {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Text {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Custom {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Line {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Path {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Image {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Icon {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Glow {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::BackdropBlur {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::BackdropBlurPath {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Overlay {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::StaticLayer {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::ScrollRaster {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::Clip {
            phase: command_phase,
            ..
        }
        | ScenePrimitive::ClipPath {
            phase: command_phase,
            ..
        } => *command_phase == phase,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Point, StaticLayerSource, Stroke, UiNodeKind};

    fn id(value: &str) -> UiId {
        UiId::owned(value.to_string())
    }

    #[test]
    fn popup_commands_escape_clips_and_render_after_regular_commands() {
        let clip_id = id("clip");
        let content_id = id("content");
        let popup_id = id("popup");
        let overlay_id = id("overlay");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(clip_id.clone(), UiNodeKind::Clip, UiRect::new(0, 0, 20, 20)).clip(
                UiRect::new(0, 0, 20, 20),
                0,
                -12,
            ),
        );
        tree.push(
            UiNode::new(
                content_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(0, 0, 10, 10),
            )
            .parent(clip_id.clone())
            .style(VisualStyle::filled(Color::WHITE)),
        );
        tree.push(
            UiNode::new(
                popup_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(20, 20, 40, 40),
            )
            .parent(clip_id)
            .style(VisualStyle::filled(Color::WHITE))
            .render_phase(RenderPhase::Popup),
        );
        tree.push(
            UiNode::new(
                overlay_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(20, 20, 40, 40),
            )
            .style(VisualStyle::filled(Color::BLACK))
            .render_phase(RenderPhase::Overlay),
        );

        let list = compile_scene(&tree);
        assert_eq!(list.commands().len(), 3);
        assert_eq!(list.commands()[0].id(), &id("clip"));
        assert_eq!(list.commands()[1].id(), &overlay_id);
        assert_eq!(list.commands()[2].id(), &popup_id);
        assert_eq!(list.commands()[2].rect(), UiRect::new(20, 8, 40, 28));
        let ScenePrimitive::Clip { commands, .. } = &list.commands()[0] else {
            panic!("expected clip command");
        };
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].id(), &content_id);
    }

    #[test]
    fn popup_clip_keeps_its_children_nested() {
        let popup_clip_id = id("popup-clip");
        let popup_child_id = id("popup-child");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                popup_clip_id.clone(),
                UiNodeKind::Clip,
                UiRect::new(10, 10, 50, 50),
            )
            .clip(UiRect::new(10, 10, 50, 50), 0, 0)
            .render_phase(RenderPhase::Popup),
        );
        tree.push(
            UiNode::new(
                popup_child_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(12, 12, 48, 48),
            )
            .parent(popup_clip_id.clone())
            .style(VisualStyle::filled(Color::WHITE))
            .render_phase(RenderPhase::Popup),
        );

        let list = compile_scene(&tree);
        assert_eq!(list.commands().len(), 1);
        let ScenePrimitive::Clip { commands, .. } = &list.commands()[0] else {
            panic!("expected popup clip command");
        };
        assert_eq!(list.commands()[0].id(), &popup_clip_id);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].id(), &popup_child_id);
    }

    #[test]
    fn popup_static_layer_translates_its_nested_commands_with_ancestor_content() {
        let ancestor_id = id("ancestor");
        let popup_layer_id = id("popup-layer");
        let popup_child_id = id("popup-layer-child");
        let mut tree = HostTree::new();
        tree.push(
            UiNode::new(
                ancestor_id.clone(),
                UiNodeKind::Clip,
                UiRect::new(0, 0, 60, 60),
            )
            .clip(UiRect::new(0, 0, 60, 60), 0, -12),
        );
        tree.push(
            UiNode::new(
                popup_layer_id.clone(),
                UiNodeKind::StaticLayer,
                UiRect::new(20, 20, 40, 40),
            )
            .parent(ancestor_id)
            .static_layer(StaticLayerSpec::new(StaticLayerSource::runtime()))
            .render_phase(RenderPhase::Popup),
        );
        tree.push(
            UiNode::new(
                popup_child_id.clone(),
                UiNodeKind::Panel,
                UiRect::new(22, 22, 38, 38),
            )
            .parent(popup_layer_id.clone())
            .style(VisualStyle::filled(Color::WHITE))
            .render_phase(RenderPhase::Popup),
        );

        let list = compile_scene(&tree);
        assert_eq!(list.commands().len(), 2);
        let ScenePrimitive::StaticLayer { rect, commands, .. } = &list.commands()[1] else {
            panic!("expected popup static layer command");
        };
        assert_eq!(list.commands()[1].id(), &popup_layer_id);
        assert_eq!(*rect, UiRect::new(20, 8, 40, 28));
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].id(), &popup_child_id);
        assert_eq!(commands[0].rect(), UiRect::new(22, 10, 38, 26));
    }

    #[test]
    fn physical_projection_scales_nested_raster_commands_and_cache_keys() {
        let text = ScenePrimitive::Text {
            id: id("text"),
            rect: UiRect::new(2, 4, 12, 14),
            text: Cow::Borrowed("DPI"),
            style: TextStyle::new(Color::WHITE, -11, 700).tracking(2),
            phase: RenderPhase::Content,
        };
        let clip = ScenePrimitive::Clip {
            id: id("clip"),
            rect: UiRect::new(1, 1, 3, 3),
            commands: vec![text],
            child_signature: 5,
            phase: RenderPhase::Content,
        };
        let list = Scene {
            commands: Arc::new(vec![ScenePrimitive::ScrollRaster {
                id: id("raster"),
                viewport: UiRect::new(0, 0, 100, 80),
                spec: ScrollRasterSpec {
                    cache_epoch: 11,
                    content_height: 200,
                    scroll_y: 4,
                    tile_height_px: 32,
                    memory_budget_bytes: 1024,
                    background_fill: Some(Color::BLACK),
                    visible_tiles: vec![0, 1],
                    prefetch_tiles: vec![2],
                    max_prefetch_tiles_per_frame: 1,
                    max_prefetch_ms_per_frame: 2,
                },
                commands: vec![clip],
                child_signature: 7,
                phase: RenderPhase::Content,
            }]),
        };

        let projected = list.project_to_physical(UiScale::new(1.5));
        let ScenePrimitive::ScrollRaster {
            viewport,
            spec,
            commands,
            child_signature,
            ..
        } = &projected.commands[0]
        else {
            panic!("expected scroll raster");
        };
        assert_eq!(*viewport, UiRect::new(0, 0, 150, 120));
        assert_eq!(spec.content_height, 300);
        assert_eq!(spec.scroll_y, 6);
        assert_eq!(spec.tile_height_px, 48);
        assert_ne!(spec.cache_epoch, 11);
        assert_ne!(*child_signature, 7);

        let ScenePrimitive::Clip {
            rect,
            commands,
            child_signature,
            ..
        } = &commands[0]
        else {
            panic!("expected clip");
        };
        assert_eq!(*rect, UiRect::new(1, 1, 5, 5));
        assert_ne!(*child_signature, 5);

        let ScenePrimitive::Text { rect, style, .. } = &commands[0] else {
            panic!("expected text");
        };
        assert_eq!(*rect, UiRect::new(3, 6, 18, 21));
        assert_eq!(style.height, -17);
        assert_eq!(style.tracking, 3);
    }

    #[test]
    fn physical_projection_scales_paths_strokes_radii_and_static_offsets() {
        let path = UiPath::new([
            UiPathCommand::MoveTo(Point::new(2, 3)),
            UiPathCommand::QuadraticTo {
                control: Point::new(4, 5),
                to: Point::new(6, 7),
            },
            UiPathCommand::Close,
        ]);
        let list = Scene {
            commands: Arc::new(vec![
                ScenePrimitive::Rect {
                    id: id("rect"),
                    rect: UiRect::new(1, 2, 11, 12),
                    style: VisualStyle::filled(Color::WHITE)
                        .radius(3)
                        .stroked(Stroke::new(Color::BLACK, 2, 255)),
                    phase: RenderPhase::Background,
                },
                ScenePrimitive::Path {
                    id: id("path"),
                    rect: UiRect::new(0, 0, 8, 8),
                    path,
                    style: PathStyle {
                        fill: None,
                        fill_alpha: 255,
                        stroke: Some(Stroke::new(Color::WHITE, 2, 255)),
                    },
                    phase: RenderPhase::Content,
                },
                ScenePrimitive::StaticLayer {
                    id: id("static"),
                    rect: UiRect::new(0, 0, 10, 10),
                    spec: StaticLayerSpec::new(StaticLayerSource::runtime()).paint_offset(-2, 3),
                    commands: Vec::new(),
                    child_signature: 13,
                    phase: RenderPhase::Background,
                },
            ]),
        };

        let projected = list.project_to_physical(UiScale::new(1.25));
        let ScenePrimitive::Rect { style, .. } = &projected.commands[0] else {
            panic!("expected rect");
        };
        assert_eq!(style.radius, 4);
        assert_eq!(style.stroke.expect("stroke").width, 3);

        let ScenePrimitive::Path { path, style, .. } = &projected.commands[1] else {
            panic!("expected path");
        };
        assert_eq!(
            path.commands(),
            &[
                UiPathCommand::MoveTo(Point::new(3, 4)),
                UiPathCommand::QuadraticTo {
                    control: Point::new(5, 6),
                    to: Point::new(8, 9),
                },
                UiPathCommand::Close,
            ]
        );
        assert_eq!(style.stroke.expect("stroke").width, 3);

        let ScenePrimitive::StaticLayer {
            spec,
            child_signature,
            ..
        } = &projected.commands[2]
        else {
            panic!("expected static layer");
        };
        assert_eq!((spec.offset_x, spec.offset_y), (-3, 4));
        assert_ne!(*child_signature, 13);
    }
}
