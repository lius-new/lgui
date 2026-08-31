use std::{borrow::Cow, path::PathBuf, sync::Arc, time::Duration};

use super::{
    ActionId, AnimationBinding, BackdropBlurStyle, Color, ComponentId, CompositingLayerSpec,
    CustomPaintStyle, IconStyle, ImageFit, LayoutSpec, OverlayStyle, PathStyle, PhysicalSize,
    RenderPhase, ScrollRasterSpec, Semantics, StaticLayerSpec, TextStyle, UiAction,
    UiActionBinding, UiActionHandler, UiEventContext, UiEventHandler, UiEventKind, UiEventPayload,
    UiId, UiInputEventBinding, UiInputEventHandler, UiPath, UiRect, VisualStyle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiNodeKind {
    Root,
    Group,
    Text,
    Image,
    Icon,
    Glow,
    BackdropBlur,
    BackdropBlurPath,
    Overlay,
    Line,
    Path,
    Ellipse,
    Panel,
    Button,
    Table,
    TableRow,
    CompositingLayer,
    StaticLayer,
    ScrollRaster,
    Clip,
    ClipPath,
    Custom(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionRole {
    None,
    Button,
    Navigation,
    Row,
    DragHandle,
    WindowDragRegion,
    Custom(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UiImageSource {
    Static(&'static str),
    File(PathBuf),
    Url(String),
    Bytes {
        key: String,
        version: u64,
        bytes: Arc<Vec<u8>>,
    },
}

impl UiImageSource {
    pub fn static_asset(source: &'static str) -> Self {
        Self::Static(source)
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self::Url(url.into())
    }

    pub fn bytes(key: impl Into<String>, version: u64, bytes: impl Into<Arc<Vec<u8>>>) -> Self {
        Self::Bytes {
            key: key.into(),
            version,
            bytes: bytes.into(),
        }
    }
}

impl From<&'static str> for UiImageSource {
    fn from(value: &'static str) -> Self {
        Self::Static(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageCachePolicy {
    NoStore,
    WhileVisible,
    Scene,
    #[default]
    Session,
    Persistent {
        max_age: Duration,
        revalidate: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageDecodePolicy {
    #[default]
    Original,
    FitTarget(PhysicalSize),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageRequest {
    source: UiImageSource,
    cache_policy: ImageCachePolicy,
    decode_policy: ImageDecodePolicy,
    priority: crate::memory::CachePriority,
    namespace: String,
    version: u64,
    sensitive: bool,
}

impl ImageRequest {
    pub fn new(source: impl Into<UiImageSource>) -> Self {
        Self {
            source: source.into(),
            cache_policy: ImageCachePolicy::Session,
            decode_policy: ImageDecodePolicy::Original,
            priority: crate::memory::CachePriority::Normal,
            namespace: "images".to_owned(),
            version: 1,
            sensitive: false,
        }
    }

    pub fn cache_policy(mut self, policy: ImageCachePolicy) -> Self {
        self.cache_policy = policy;
        self
    }

    pub fn decode_policy(mut self, policy: ImageDecodePolicy) -> Self {
        self.decode_policy = policy;
        self
    }

    pub fn priority(mut self, priority: crate::memory::CachePriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    pub fn version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }

    pub fn sensitive(mut self, sensitive: bool) -> Self {
        self.sensitive = sensitive;
        self
    }

    pub fn source(&self) -> &UiImageSource {
        &self.source
    }

    pub const fn cache_policy_value(&self) -> ImageCachePolicy {
        self.cache_policy
    }

    pub const fn decode_policy_value(&self) -> ImageDecodePolicy {
        self.decode_policy
    }

    pub const fn priority_value(&self) -> crate::memory::CachePriority {
        self.priority
    }

    pub fn namespace_value(&self) -> &str {
        &self.namespace
    }

    pub const fn version_value(&self) -> u64 {
        self.version
    }

    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    pub fn cache_key(&self) -> String {
        let source = match &self.source {
            UiImageSource::Static(key) => format!("asset:{key}"),
            UiImageSource::File(path) => format!("file:{}", path.display()),
            UiImageSource::Url(url) => format!("url:{url}"),
            UiImageSource::Bytes { key, version, .. } => format!("bytes:{key}:{version}"),
        };
        format!("{}:{source}:{}", self.namespace, self.version)
    }
}

impl From<UiImageSource> for ImageRequest {
    fn from(source: UiImageSource) -> Self {
        Self::new(source)
    }
}

impl From<&'static str> for ImageRequest {
    fn from(source: &'static str) -> Self {
        Self::new(source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventPolicy {
    pub hover: bool,
    pub press: bool,
    pub focus: bool,
}

impl EventPolicy {
    pub const NONE: Self = Self {
        hover: false,
        press: false,
        focus: false,
    };

    pub const INTERACTIVE: Self = Self {
        hover: true,
        press: true,
        focus: true,
    };
}

#[derive(Clone)]
pub struct UiNode {
    pub id: UiId,
    pub component_owner: Option<ComponentId>,
    pub parent: Option<UiId>,
    pub kind: UiNodeKind,
    pub layout_rect: UiRect,
    pub hit_rect: UiRect,
    pub paint_bounds: UiRect,
    pub ime_cursor_rect: Option<UiRect>,
    pub interaction: InteractionRole,
    pub semantics: Option<Semantics>,
    pub click_capture_handler: Option<UiEventHandler>,
    pub click_handler: Option<UiEventHandler>,
    pub input_event_handlers: Vec<UiInputEventBinding>,
    pub click_action: Option<UiAction>,
    pub wheel_action: Option<UiAction>,
    pub action_target: Option<UiId>,
    pub action_handlers: Vec<UiActionBinding>,
    pub event_policy: EventPolicy,
    pub auto_focus: bool,
    pub focus_scope: bool,
    pub animation_bindings: Vec<AnimationBinding>,
    pub animation_targets: Vec<(super::AnimProperty, bool)>,
    pub animation_outset: (f32, f32),
    pub layout: LayoutSpec,
    pub style: VisualStyle,
    pub path: Option<UiPath>,
    pub path_style: PathStyle,
    pub image_request: Option<ImageRequest>,
    pub image_fit: ImageFit,
    pub icon_key: Option<&'static str>,
    pub icon_style: IconStyle,
    pub glow: Option<(Color, u8)>,
    pub backdrop_blur_style: Option<BackdropBlurStyle>,
    pub overlay_style: Option<OverlayStyle>,
    pub custom_style: Option<CustomPaintStyle>,
    pub compositing_layer: Option<CompositingLayerSpec>,
    pub static_layer: Option<StaticLayerSpec>,
    pub scroll_raster: Option<ScrollRasterSpec>,
    pub clip_rect: Option<UiRect>,
    pub content_offset: (f32, f32),
    pub text: Option<Cow<'static, str>>,
    pub text_style: Option<TextStyle>,
    pub render_phase: RenderPhase,
    pub children: Vec<UiId>,
}

impl UiNode {
    pub fn new(id: UiId, kind: UiNodeKind, layout_rect: UiRect) -> Self {
        Self {
            id,
            component_owner: None,
            parent: None,
            kind,
            layout_rect,
            hit_rect: layout_rect,
            paint_bounds: layout_rect,
            ime_cursor_rect: None,
            interaction: InteractionRole::None,
            semantics: None,
            click_capture_handler: None,
            click_handler: None,
            input_event_handlers: Vec::new(),
            click_action: None,
            wheel_action: None,
            action_target: None,
            action_handlers: Vec::new(),
            event_policy: EventPolicy::NONE,
            auto_focus: false,
            focus_scope: false,
            animation_bindings: Vec::new(),
            animation_targets: Vec::new(),
            animation_outset: (0.0, 0.0),
            layout: LayoutSpec::default(),
            style: VisualStyle::default(),
            path: None,
            path_style: PathStyle::default(),
            image_request: None,
            image_fit: ImageFit::Contain,
            icon_key: None,
            icon_style: IconStyle::new(Color::WHITE),
            glow: None,
            backdrop_blur_style: None,
            overlay_style: None,
            custom_style: None,
            compositing_layer: None,
            static_layer: None,
            scroll_raster: None,
            clip_rect: None,
            content_offset: (0.0, 0.0),
            text: None,
            text_style: None,
            render_phase: RenderPhase::Content,
            children: Vec::new(),
        }
    }

    #[cfg(any(
        test,
        feature = "backend-winit",
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        all(feature = "backend-win32", feature = "renderer-skia")
    ))]
    pub(crate) fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.id.as_str().len())
            .saturating_add(
                self.parent
                    .as_ref()
                    .map_or(0, |parent| parent.as_str().len()),
            )
            .saturating_add(
                self.children
                    .capacity()
                    .saturating_mul(std::mem::size_of::<UiId>()),
            )
            .saturating_add(self.children.iter().map(|id| id.as_str().len()).sum())
            .saturating_add(self.text.as_deref().map_or(0, str::len))
            .saturating_add(
                self.input_event_handlers
                    .capacity()
                    .saturating_mul(std::mem::size_of::<UiInputEventBinding>()),
            )
            .saturating_add(
                self.action_handlers
                    .capacity()
                    .saturating_mul(std::mem::size_of::<UiActionBinding>()),
            )
            .saturating_add(
                self.animation_bindings
                    .capacity()
                    .saturating_mul(std::mem::size_of::<AnimationBinding>()),
            )
            .saturating_add(
                self.animation_targets
                    .capacity()
                    .saturating_mul(std::mem::size_of::<(super::AnimProperty, bool)>()),
            )
    }

    pub(crate) fn projection_eq(&self, other: &Self) -> bool {
        self.parent == other.parent
            && self.kind == other.kind
            && self.layout_rect == other.layout_rect
            && self.hit_rect == other.hit_rect
            && self.paint_bounds == other.paint_bounds
            && self.ime_cursor_rect == other.ime_cursor_rect
            && self.interaction == other.interaction
            && self.semantics == other.semantics
            && self.event_policy == other.event_policy
            && self.auto_focus == other.auto_focus
            && self.focus_scope == other.focus_scope
            && self.animation_bindings == other.animation_bindings
            && self.animation_targets == other.animation_targets
            && self.animation_outset == other.animation_outset
            && self.layout == other.layout
            && self.style == other.style
            && self.path == other.path
            && self.path_style == other.path_style
            && self.image_request == other.image_request
            && self.image_fit == other.image_fit
            && self.icon_key == other.icon_key
            && self.icon_style == other.icon_style
            && self.glow == other.glow
            && self.backdrop_blur_style == other.backdrop_blur_style
            && self.overlay_style == other.overlay_style
            && self.custom_style == other.custom_style
            && self.compositing_layer == other.compositing_layer
            && self.static_layer == other.static_layer
            && self.scroll_raster == other.scroll_raster
            && self.clip_rect == other.clip_rect
            && self.content_offset == other.content_offset
            && self.text == other.text
            && self.text_style == other.text_style
            && self.render_phase == other.render_phase
    }

    pub fn parent(mut self, parent: UiId) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn hit_rect(mut self, rect: UiRect) -> Self {
        self.hit_rect = rect;
        self
    }

    pub fn paint_bounds(mut self, rect: UiRect) -> Self {
        self.paint_bounds = rect;
        self
    }

    pub fn ime_cursor_rect(mut self, rect: UiRect) -> Self {
        self.ime_cursor_rect = Some(rect);
        self
    }

    pub fn interaction(mut self, interaction: InteractionRole) -> Self {
        self.interaction = interaction;
        if interaction != InteractionRole::None {
            self.event_policy = EventPolicy::INTERACTIVE;
        }
        self
    }

    pub fn semantics(mut self, semantics: Semantics) -> Self {
        self.semantics = Some(semantics);
        self
    }

    pub fn on_click<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_click_handler(std::sync::Arc::new(handler))
    }

    pub fn on_click_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_handler = Some(handler);
        self
    }

    pub(crate) fn component_owner(mut self, owner: ComponentId) -> Self {
        self.component_owner = Some(owner);
        self
    }

    pub fn on_click_capture_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_capture_handler = Some(handler);
        self
    }

    pub fn on_click_capture<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_click_capture_handler(Arc::new(handler))
    }

    pub fn on_event_handler(
        mut self,
        kind: UiEventKind,
        capture: bool,
        handler: UiInputEventHandler,
    ) -> Self {
        self.input_event_handlers.push(UiInputEventBinding {
            kind,
            capture,
            handler,
        });
        match kind {
            UiEventKind::PointerMove => self.event_policy.hover = true,
            UiEventKind::Click | UiEventKind::PointerDown | UiEventKind::PointerUp => {
                self.event_policy.press = true;
            }
            UiEventKind::KeyDown
            | UiEventKind::KeyUp
            | UiEventKind::Input
            | UiEventKind::CompositionStart
            | UiEventKind::CompositionUpdate
            | UiEventKind::CompositionEnd
            | UiEventKind::Focus
            | UiEventKind::Blur
            | UiEventKind::Change => self.event_policy.focus = true,
            UiEventKind::Wheel => {}
        }
        self
    }

    pub fn on_event<F>(self, kind: UiEventKind, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync + 'static,
    {
        self.on_event_handler(kind, false, Arc::new(handler))
    }

    pub fn on_event_capture<F>(self, kind: UiEventKind, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync + 'static,
    {
        self.on_event_handler(kind, true, Arc::new(handler))
    }

    pub fn wheel_action(mut self, action: UiAction) -> Self {
        self.wheel_action = Some(action);
        self
    }

    pub fn click_action(mut self, action: UiAction) -> Self {
        self.click_action = Some(action);
        self
    }

    pub fn action_target(mut self, target: UiId) -> Self {
        self.action_target = Some(target);
        self
    }

    pub fn on_action<F>(self, id: impl Into<ActionId>, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiAction) + Send + Sync + 'static,
    {
        self.on_action_handler(id, Arc::new(handler))
    }

    pub fn on_action_handler(mut self, id: impl Into<ActionId>, handler: UiActionHandler) -> Self {
        self.action_handlers.push(UiActionBinding {
            id: id.into(),
            handler,
        });
        self
    }

    pub fn event_policy(mut self, policy: EventPolicy) -> Self {
        self.event_policy = policy;
        self
    }

    pub fn auto_focus(mut self, enabled: bool) -> Self {
        self.auto_focus = enabled;
        self
    }

    pub fn focus_scope(mut self, enabled: bool) -> Self {
        self.focus_scope = enabled;
        self
    }

    pub fn animation(mut self, binding: AnimationBinding) -> Self {
        self.animation_bindings.push(binding);
        self
    }

    pub fn animation_target(mut self, property: super::AnimProperty, active: bool) -> Self {
        self.animation_targets.push((property, active));
        self
    }

    pub fn animation_outset(mut self, x: f32, y: f32) -> Self {
        self.animation_outset = (x, y);
        self
    }

    pub fn layout(mut self, layout: LayoutSpec) -> Self {
        self.layout = layout;
        self
    }

    pub fn style(mut self, style: VisualStyle) -> Self {
        self.style = style;
        self
    }

    pub fn path(mut self, path: UiPath, style: PathStyle) -> Self {
        self.path = Some(path);
        self.path_style = style;
        self
    }

    pub fn text(mut self, value: impl Into<Cow<'static, str>>, style: TextStyle) -> Self {
        self.text = Some(value.into());
        self.text_style = Some(style);
        self
    }

    pub fn image(mut self, source: impl Into<UiImageSource>, fit: ImageFit) -> Self {
        self.image_request = Some(ImageRequest::new(source));
        self.image_fit = fit;
        self
    }

    pub fn image_request(mut self, request: impl Into<ImageRequest>, fit: ImageFit) -> Self {
        self.image_request = Some(request.into());
        self.image_fit = fit;
        self
    }

    pub fn icon(mut self, key: &'static str) -> Self {
        self.icon_key = Some(key);
        self
    }

    pub fn icon_style(mut self, style: IconStyle) -> Self {
        self.icon_style = style;
        self
    }

    pub fn glow(mut self, color: Color, alpha: u8) -> Self {
        self.glow = Some((color, alpha));
        self
    }

    pub fn overlay(mut self, style: OverlayStyle) -> Self {
        self.overlay_style = Some(style);
        self
    }

    pub fn backdrop_blur(mut self, style: BackdropBlurStyle) -> Self {
        self.backdrop_blur_style = Some(style);
        self
    }

    pub fn custom_style(mut self, style: CustomPaintStyle) -> Self {
        self.custom_style = Some(style);
        self
    }

    pub fn static_layer(mut self, spec: StaticLayerSpec) -> Self {
        self.static_layer = Some(spec);
        self
    }

    pub fn compositing_layer(mut self, spec: CompositingLayerSpec) -> Self {
        self.compositing_layer = Some(spec);
        self
    }

    pub fn scroll_raster(mut self, spec: ScrollRasterSpec) -> Self {
        self.clip_rect = Some(self.layout_rect);
        self.content_offset = (0.0, -spec.scroll_y);
        self.scroll_raster = Some(spec);
        self
    }

    pub fn clip(mut self, rect: UiRect, offset_x: f32, offset_y: f32) -> Self {
        self.clip_rect = Some(rect);
        self.content_offset = (offset_x, offset_y);
        self
    }

    pub fn render_phase(mut self, phase: RenderPhase) -> Self {
        self.render_phase = phase;
        self
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.layout_rect = self.layout_rect.translate(x, y);
        self.hit_rect = self.hit_rect.translate(x, y);
        self.paint_bounds = self.paint_bounds.translate(x, y);
        self.ime_cursor_rect = self.ime_cursor_rect.map(|rect| rect.translate(x, y));
        self.clip_rect = self.clip_rect.map(|rect| rect.translate(x, y));
        self.path = self.path.map(|path| translate_path(&path, x, y));
        self
    }
}

fn translate_path(path: &UiPath, x: f32, y: f32) -> UiPath {
    let translate = |point: super::Point| super::Point::new(point.x + x, point.y + y);
    UiPath::new(path.commands().iter().map(|command| match *command {
        super::UiPathCommand::MoveTo(point) => super::UiPathCommand::MoveTo(translate(point)),
        super::UiPathCommand::LineTo(point) => super::UiPathCommand::LineTo(translate(point)),
        super::UiPathCommand::QuadraticTo { control, to } => super::UiPathCommand::QuadraticTo {
            control: translate(control),
            to: translate(to),
        },
        super::UiPathCommand::CubicTo {
            control1,
            control2,
            to,
        } => super::UiPathCommand::CubicTo {
            control1: translate(control1),
            control2: translate(control2),
            to: translate(to),
        },
        super::UiPathCommand::Close => super::UiPathCommand::Close,
    }))
}
