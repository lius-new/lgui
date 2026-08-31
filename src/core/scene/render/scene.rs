use super::{primitive::*, *};

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub(super) commands: Arc<Vec<ScenePrimitive>>,
}

const SCROLL_RASTER_COMMAND_CACHE_LIMIT: usize = 8;

#[derive(Clone, Hash, PartialEq, Eq)]
struct ScrollRasterCommandCacheKey {
    id: String,
    cache_epoch: u64,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    content_height: u32,
}

#[derive(Clone)]
struct ScrollRasterCommandSnapshot {
    commands: Vec<ScenePrimitive>,
    child_signature: u64,
    last_used: u64,
    bytes: usize,
}

struct ScrollRasterCommandCache {
    entries: HashMap<ScrollRasterCommandCacheKey, ScrollRasterCommandSnapshot>,
    tick: u64,
    bytes: usize,
    budget_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Default for ScrollRasterCommandCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            tick: 0,
            bytes: 0,
            budget_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
}

fn scroll_raster_command_cache() -> &'static Mutex<ScrollRasterCommandCache> {
    static CACHE: OnceLock<Mutex<ScrollRasterCommandCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ScrollRasterCommandCache::default()))
}

pub(crate) fn scroll_raster_command_cache_usage() -> crate::memory::CacheUsage {
    let cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    crate::memory::CacheUsage {
        rebuildable_bytes: cache.bytes,
        cpu_bytes: cache.bytes,
        entries: cache.entries.len(),
        hits: cache.hits,
        misses: cache.misses,
        evictions: cache.evictions,
        largest_entry_bytes: cache
            .entries
            .values()
            .map(|entry| entry.bytes)
            .max()
            .unwrap_or(0),
        ..Default::default()
    }
}

pub(crate) fn trim_scroll_raster_command_cache(target_bytes: usize) -> usize {
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    let before = cache.bytes;
    evict_scroll_raster_commands(&mut cache, target_bytes, SCROLL_RASTER_COMMAND_CACHE_LIMIT);
    before.saturating_sub(cache.bytes)
}

pub(crate) fn set_scroll_raster_command_cache_budget(budget_bytes: usize) {
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    cache.budget_bytes = budget_bytes;
    let budget = cache.budget_bytes;
    evict_scroll_raster_commands(&mut cache, budget, SCROLL_RASTER_COMMAND_CACHE_LIMIT);
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

    pub fn image_requests(&self) -> Vec<ImageRequest> {
        let mut requests = Vec::new();
        collect_image_requests(self.commands(), &mut requests);
        requests
    }

    pub fn estimated_bytes(&self) -> usize {
        estimate_scene_commands_bytes(self.commands())
    }

    pub(super) fn move_popup_commands_to_end(&mut self) {
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

    pub(crate) fn patch_compositing_layer_spec(
        &mut self,
        id: &UiId,
        spec: CompositingLayerSpec,
    ) -> bool {
        let commands = Arc::make_mut(&mut self.commands);
        patch_compositing_layer_spec(commands.as_mut_slice(), id, spec)
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

fn collect_image_requests(commands: &[ScenePrimitive], requests: &mut Vec<ImageRequest>) {
    for command in commands {
        match command {
            ScenePrimitive::Image { request, .. } => requests.push(request.clone()),
            ScenePrimitive::CompositingLayer { commands, .. }
            | ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => {
                collect_image_requests(commands, requests);
            }
            _ => {}
        }
    }
}

pub(crate) fn patch_compositing_layer_spec(
    commands: &mut [ScenePrimitive],
    source: &UiId,
    next_spec: CompositingLayerSpec,
) -> bool {
    for command in commands {
        let nested = match command {
            ScenePrimitive::CompositingLayer {
                id, spec, commands, ..
            } => {
                if id == source {
                    *spec = next_spec;
                    return true;
                }
                Some(commands)
            }
            ScenePrimitive::StaticLayer { commands, .. }
            | ScenePrimitive::ScrollRaster { commands, .. }
            | ScenePrimitive::Clip { commands, .. }
            | ScenePrimitive::ClipPath { commands, .. } => Some(commands),
            _ => None,
        };
        if nested.is_some_and(|commands| patch_compositing_layer_spec(commands, source, next_spec))
        {
            return true;
        }
    }
    false
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            start: scale.physical_ui_point(*start),
            end: scale.physical_ui_point(*end),
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
            rect: scale.physical_ui_rect(*rect),
            path: project_path(path, scale),
            style: project_path_style(*style, scale),
            phase: *phase,
        },
        ScenePrimitive::Image {
            id,
            rect,
            source,
            request,
            fit,
            phase,
        } => ScenePrimitive::Image {
            id: id.clone(),
            rect: scale.physical_ui_rect(*rect),
            source: source.clone(),
            request: request.clone(),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
            style: style.clone(),
            phase: *phase,
        },
        ScenePrimitive::CompositingLayer {
            id,
            rect,
            spec,
            commands,
            content_signature,
            phase,
        } => {
            let mut spec = *spec;
            spec.transform = spec.transform.project_to_physical(scale);
            ScenePrimitive::CompositingLayer {
                id: id.clone(),
                rect: scale.physical_ui_rect(*rect),
                spec,
                commands: commands
                    .iter()
                    .map(|command| project_command(command, scale))
                    .collect(),
                content_signature: content_signature ^ signature_scale,
                phase: *phase,
            }
        }
        ScenePrimitive::StaticLayer {
            id,
            rect,
            spec,
            commands,
            child_signature,
            phase,
        } => {
            let mut spec = spec.clone();
            spec.offset_x = scale.physical_ui_value(spec.offset_x);
            spec.offset_y = scale.physical_ui_value(spec.offset_y);
            ScenePrimitive::StaticLayer {
                id: id.clone(),
                rect: scale.physical_ui_rect(*rect),
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
            spec.content_height = scale.physical_ui_length(spec.content_height);
            spec.scroll_y = scale.physical_ui_value(spec.scroll_y);
            spec.tile_height_px = scale.physical_ui_length(spec.tile_height_px);
            ScenePrimitive::ScrollRaster {
                id: id.clone(),
                viewport: scale.physical_ui_rect(*viewport),
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
            rect: scale.physical_ui_rect(*rect),
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
            rect: scale.physical_ui_rect(*rect),
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
    stroke.width = scale.physical_ui_length(stroke.width);
    stroke
}

fn project_visual_style(mut style: VisualStyle, scale: UiScale) -> VisualStyle {
    style.radius = scale.physical_ui_length(style.radius);
    style.stroke = style.stroke.map(|stroke| project_stroke(stroke, scale));
    style
}

fn project_path_style(mut style: PathStyle, scale: UiScale) -> PathStyle {
    style.stroke = style.stroke.map(|stroke| project_stroke(stroke, scale));
    style
}

fn project_text_style(mut style: TextStyle, scale: UiScale) -> TextStyle {
    style.height = scale.physical_ui_signed_length(style.height);
    style.tracking = scale.physical_ui_value(style.tracking);
    style
}

fn project_backdrop_blur_style(mut style: BackdropBlurStyle, scale: UiScale) -> BackdropBlurStyle {
    style.source_rect = scale.physical_ui_rect(style.source_rect);
    style.radius = scale.physical_ui_length(style.radius);
    style
}

fn project_path(path: &UiPath, scale: UiScale) -> UiPath {
    UiPath::new(path.commands().iter().map(|command| match *command {
        UiPathCommand::MoveTo(point) => UiPathCommand::MoveTo(scale.physical_ui_point(point)),
        UiPathCommand::LineTo(point) => UiPathCommand::LineTo(scale.physical_ui_point(point)),
        UiPathCommand::QuadraticTo { control, to } => UiPathCommand::QuadraticTo {
            control: scale.physical_ui_point(control),
            to: scale.physical_ui_point(to),
        },
        UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => UiPathCommand::CubicTo {
            control1: scale.physical_ui_point(control1),
            control2: scale.physical_ui_point(control2),
            to: scale.physical_ui_point(to),
        },
        UiPathCommand::Close => UiPathCommand::Close,
    }))
}

pub(super) fn load_scroll_raster_command_snapshot(
    node: &UiNode,
    spec: &ScrollRasterSpec,
) -> Option<(Vec<ScenePrimitive>, u64)> {
    let key = scroll_raster_command_cache_key(node, spec);
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    cache.tick = cache.tick.saturating_add(1);
    let tick = cache.tick;
    if !cache.entries.contains_key(&key) {
        cache.misses = cache.misses.saturating_add(1);
        return None;
    }
    cache.hits = cache.hits.saturating_add(1);
    let snapshot = cache
        .entries
        .get_mut(&key)
        .expect("scroll raster command cache key vanished");
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

pub(super) fn store_scroll_raster_command_snapshot(
    node: &UiNode,
    spec: &ScrollRasterSpec,
    commands: Vec<ScenePrimitive>,
    child_signature: u64,
) {
    let key = scroll_raster_command_cache_key(node, spec);
    let bytes = estimate_scene_commands_bytes(&commands);
    let mut cache = scroll_raster_command_cache()
        .lock()
        .expect("scroll raster command cache poisoned");
    if bytes > cache.budget_bytes {
        return;
    }
    cache.tick = cache.tick.saturating_add(1);
    let tick = cache.tick;
    if let Some(previous) = cache.entries.remove(&key) {
        cache.bytes = cache.bytes.saturating_sub(previous.bytes);
    }
    cache.bytes = cache.bytes.saturating_add(bytes);
    cache.entries.insert(
        key,
        ScrollRasterCommandSnapshot {
            commands,
            child_signature,
            last_used: tick,
            bytes,
        },
    );
    let budget = cache.budget_bytes;
    evict_scroll_raster_commands(&mut cache, budget, SCROLL_RASTER_COMMAND_CACHE_LIMIT);
}

fn evict_scroll_raster_commands(
    cache: &mut ScrollRasterCommandCache,
    target_bytes: usize,
    target_entries: usize,
) {
    while cache.bytes > target_bytes || cache.entries.len() > target_entries {
        let Some(oldest_key) = cache
            .entries
            .iter()
            .min_by_key(|(_, snapshot)| snapshot.last_used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(entry) = cache.entries.remove(&oldest_key) {
            cache.bytes = cache.bytes.saturating_sub(entry.bytes);
            cache.evictions = cache.evictions.saturating_add(1);
        }
    }
}

pub(crate) fn estimate_scene_commands_bytes(commands: &[ScenePrimitive]) -> usize {
    std::mem::size_of_val(commands).saturating_add(
        commands
            .iter()
            .map(estimate_scene_primitive_dynamic_bytes)
            .sum::<usize>(),
    )
}

fn estimate_scene_primitive_dynamic_bytes(command: &ScenePrimitive) -> usize {
    match command {
        ScenePrimitive::Text { text, .. } => text.len(),
        ScenePrimitive::Path { path, .. } | ScenePrimitive::BackdropBlurPath { path, .. } => path
            .commands()
            .len()
            .saturating_mul(std::mem::size_of::<UiPathCommand>()),
        ScenePrimitive::Image { source, .. } => match source {
            UiImageSource::Bytes { bytes, key, .. } => bytes.len().saturating_add(key.len()),
            UiImageSource::Url(value) => value.len(),
            UiImageSource::File(value) => value.as_os_str().len(),
            UiImageSource::Static(value) => value.len(),
        },
        ScenePrimitive::CompositingLayer { commands, .. }
        | ScenePrimitive::StaticLayer { commands, .. }
        | ScenePrimitive::ScrollRaster { commands, .. }
        | ScenePrimitive::Clip { commands, .. }
        | ScenePrimitive::ClipPath { commands, .. } => estimate_scene_commands_bytes(commands),
        _ => 0,
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
        left: normalized_f32_bits(rect.left),
        top: normalized_f32_bits(rect.top),
        right: normalized_f32_bits(rect.right),
        bottom: normalized_f32_bits(rect.bottom),
        content_height: normalized_f32_bits(spec.content_height),
    }
}
