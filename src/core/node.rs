use std::{borrow::Cow, path::PathBuf, sync::Arc};

use super::{
    ActionId, AnimationBinding, BackdropBlurStyle, Color, ComponentId, CustomPaintStyle, IconStyle,
    ImageFit, LayoutSpec, OverlayStyle, PathStyle, RenderPhase, ScrollRasterSpec, StaticLayerSpec,
    TextStyle, UiAction, UiActionBinding, UiActionHandler, UiEventContext, UiEventHandler,
    UiEventKind, UiEventPayload, UiId, UiInputEventBinding, UiInputEventHandler, UiPath, UiRect,
    VisualStyle,
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
    pub interaction: InteractionRole,
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
    pub animation_outset: (i32, i32),
    pub layout: LayoutSpec,
    pub style: VisualStyle,
    pub path: Option<UiPath>,
    pub path_style: PathStyle,
    pub image_source: Option<UiImageSource>,
    pub image_fit: ImageFit,
    pub icon_key: Option<&'static str>,
    pub icon_style: IconStyle,
    pub glow: Option<(Color, u8)>,
    pub backdrop_blur_style: Option<BackdropBlurStyle>,
    pub overlay_style: Option<OverlayStyle>,
    pub custom_style: Option<CustomPaintStyle>,
    pub static_layer: Option<StaticLayerSpec>,
    pub scroll_raster: Option<ScrollRasterSpec>,
    pub clip_rect: Option<UiRect>,
    pub content_offset: (i32, i32),
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
            interaction: InteractionRole::None,
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
            animation_outset: (0, 0),
            layout: LayoutSpec::default(),
            style: VisualStyle::default(),
            path: None,
            path_style: PathStyle::default(),
            image_source: None,
            image_fit: ImageFit::Contain,
            icon_key: None,
            icon_style: IconStyle::new(Color::WHITE),
            glow: None,
            backdrop_blur_style: None,
            overlay_style: None,
            custom_style: None,
            static_layer: None,
            scroll_raster: None,
            clip_rect: None,
            content_offset: (0, 0),
            text: None,
            text_style: None,
            render_phase: RenderPhase::Content,
            children: Vec::new(),
        }
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

    pub fn interaction(mut self, interaction: InteractionRole) -> Self {
        self.interaction = interaction;
        if interaction != InteractionRole::None {
            self.event_policy = EventPolicy::INTERACTIVE;
        }
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

    pub fn animation_outset(mut self, x: i32, y: i32) -> Self {
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
        self.image_source = Some(source.into());
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

    pub fn scroll_raster(mut self, spec: ScrollRasterSpec) -> Self {
        self.clip_rect = Some(self.layout_rect);
        self.content_offset = (0, -spec.scroll_y);
        self.scroll_raster = Some(spec);
        self
    }

    pub fn clip(mut self, rect: UiRect, offset_x: i32, offset_y: i32) -> Self {
        self.clip_rect = Some(rect);
        self.content_offset = (offset_x, offset_y);
        self
    }

    pub fn render_phase(mut self, phase: RenderPhase) -> Self {
        self.render_phase = phase;
        self
    }

    pub fn translate(mut self, x: i32, y: i32) -> Self {
        self.layout_rect = self.layout_rect.translate(x, y);
        self.hit_rect = self.hit_rect.translate(x, y);
        self.paint_bounds = self.paint_bounds.translate(x, y);
        self.clip_rect = self.clip_rect.map(|rect| rect.translate(x, y));
        self.path = self.path.map(|path| translate_path(&path, x, y));
        self
    }
}

fn translate_path(path: &UiPath, x: i32, y: i32) -> UiPath {
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
