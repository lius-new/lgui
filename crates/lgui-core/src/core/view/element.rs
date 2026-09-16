use std::{borrow::Cow, future::Future, sync::Arc};

use super::{
    async_handler, AnimationBinding, BackdropBlurStyle, Color, ComponentId, CompositingLayerSpec,
    CustomPaintStyle, EventPolicy, HostTreeBuilder, IconStyle, ImageFit, ImageRequest,
    InteractionRole, LayoutSpec, OverlayStyle, PathStyle, RenderPhase, ScrollRasterSpec,
    StaticLayerSpec, TextStyle, UiAction, UiAsyncContext, UiEventContext, UiEventHandler,
    UiEventKind, UiEventPayload, UiId, UiImageSource, UiInputEventHandler, UiNode, UiNodeKind,
    UiPath, UiRect, VisualStyle,
};

#[derive(Clone)]
pub struct UiElement {
    node: UiNode,
    children: Arc<Vec<UiElement>>,
    component_boundary: Option<ComponentBoundary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ComponentBoundary {
    pub id: ComponentId,
    pub retain_children: bool,
}

pub trait UiComponent {
    fn render(self) -> UiElement;
}

impl UiComponent for UiElement {
    fn render(self) -> UiElement {
        self
    }
}

impl UiElement {
    pub fn new(id: UiId, kind: UiNodeKind, rect: UiRect) -> Self {
        Self {
            node: UiNode::new(id, kind, rect),
            children: Arc::new(Vec::new()),
            component_boundary: None,
        }
    }

    #[cfg(any(test, feature = "backend-winit", feature = "backend-win32"))]
    pub(crate) fn estimated_bytes(&self) -> usize {
        self.node
            .estimated_bytes()
            .saturating_add(
                self.children
                    .capacity()
                    .saturating_mul(std::mem::size_of::<UiElement>()),
            )
            .saturating_add(
                self.children
                    .iter()
                    .map(UiElement::estimated_bytes)
                    .sum::<usize>(),
            )
    }

    pub fn group(id: UiId, rect: UiRect) -> Self {
        Self::new(id, UiNodeKind::Group, rect)
    }

    pub fn panel(id: UiId, rect: UiRect, style: VisualStyle) -> Self {
        Self::new(id, UiNodeKind::Panel, rect).style(style)
    }

    pub fn button(id: UiId, rect: UiRect, style: VisualStyle) -> Self {
        Self::new(id, UiNodeKind::Button, rect)
            .style(style)
            .interaction(InteractionRole::Button)
    }

    pub fn text(
        id: UiId,
        rect: UiRect,
        text: impl Into<Cow<'static, str>>,
        style: TextStyle,
    ) -> Self {
        Self::new(id, UiNodeKind::Text, rect).text_content(text, style)
    }

    pub fn custom(id: UiId, rect: UiRect, key: &'static str) -> Self {
        Self::new(id, UiNodeKind::Custom(key), rect)
    }

    pub fn custom_paint(
        id: UiId,
        rect: UiRect,
        key: &'static str,
        style: CustomPaintStyle,
    ) -> Self {
        Self::custom(id, rect, key).custom_style(style)
    }

    pub fn image(id: UiId, rect: UiRect, source: &'static str, fit: ImageFit) -> Self {
        Self::new(id, UiNodeKind::Image, rect).image_source(source, fit)
    }

    pub fn file_image(
        id: UiId,
        rect: UiRect,
        source: impl Into<std::path::PathBuf>,
        fit: ImageFit,
    ) -> Self {
        Self::new(id, UiNodeKind::Image, rect).image_source(UiImageSource::file(source), fit)
    }

    pub fn requested_image(
        id: UiId,
        rect: UiRect,
        request: impl Into<ImageRequest>,
        fit: ImageFit,
    ) -> Self {
        Self::new(id, UiNodeKind::Image, rect).image_request(request, fit)
    }

    pub fn icon(id: UiId, rect: UiRect, key: &'static str) -> Self {
        Self::new(id, UiNodeKind::Icon, rect).icon_key(key)
    }

    pub fn glow(id: UiId, rect: UiRect, color: Color, alpha: u8) -> Self {
        Self::new(id, UiNodeKind::Glow, rect).glow_effect(color, alpha)
    }

    pub fn backdrop_blur(id: UiId, rect: UiRect, style: BackdropBlurStyle) -> Self {
        Self::new(id, UiNodeKind::BackdropBlur, rect).backdrop_blur_style(style)
    }

    pub fn backdrop_blur_path(
        id: UiId,
        rect: UiRect,
        path: UiPath,
        style: BackdropBlurStyle,
    ) -> Self {
        Self::new(id, UiNodeKind::BackdropBlurPath, rect)
            .path_content(path, PathStyle::default())
            .backdrop_blur_style(style)
    }

    pub fn overlay(id: UiId, rect: UiRect, style: OverlayStyle) -> Self {
        Self::new(id, UiNodeKind::Overlay, rect).overlay_style(style)
    }

    pub fn static_layer(id: UiId, rect: UiRect, spec: StaticLayerSpec) -> Self {
        Self::new(id, UiNodeKind::StaticLayer, rect).static_layer_spec(spec)
    }

    pub fn compositing_layer(id: UiId, rect: UiRect, spec: CompositingLayerSpec) -> Self {
        Self::new(id, UiNodeKind::CompositingLayer, rect).compositing_layer_spec(spec)
    }

    pub fn scroll_raster(id: UiId, viewport: UiRect, spec: ScrollRasterSpec) -> Self {
        Self::new(id, UiNodeKind::ScrollRaster, viewport).scroll_raster_spec(spec)
    }

    pub fn clip(id: UiId, rect: UiRect, offset_x: f32, offset_y: f32) -> Self {
        Self::new(id, UiNodeKind::Clip, rect).clip_content(rect, offset_x, offset_y)
    }

    pub fn clip_path(id: UiId, rect: UiRect, path: UiPath) -> Self {
        Self::new(id, UiNodeKind::ClipPath, rect).path_content(path, PathStyle::default())
    }

    pub fn line(id: UiId, rect: UiRect, style: VisualStyle) -> Self {
        Self::new(id, UiNodeKind::Line, rect).style(style)
    }

    pub fn path(id: UiId, rect: UiRect, path: UiPath, style: PathStyle) -> Self {
        Self::new(id, UiNodeKind::Path, rect).path_content(path, style)
    }

    pub fn ellipse(id: UiId, rect: UiRect, style: VisualStyle) -> Self {
        Self::new(id, UiNodeKind::Ellipse, rect).style(style)
    }

    pub fn node(&self) -> &UiNode {
        &self.node
    }

    pub fn children_ref(&self) -> &[UiElement] {
        self.children.as_slice()
    }

    pub fn child(mut self, child: UiElement) -> Self {
        Arc::make_mut(&mut self.children).push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = UiElement>) -> Self {
        Arc::make_mut(&mut self.children).extend(children);
        self
    }

    pub fn interaction(mut self, interaction: InteractionRole) -> Self {
        self.node = self.node.interaction(interaction);
        self
    }

    pub fn semantics(mut self, semantics: super::Semantics) -> Self {
        self.node = self.node.semantics(semantics);
        self
    }

    /// Marks this element as a native window drag region.
    ///
    /// Interactive descendants remain clickable and automatically take precedence
    /// over the drag region during platform hit testing.
    pub fn window_drag_region(mut self) -> Self {
        self.node = self
            .node
            .interaction(InteractionRole::WindowDragRegion)
            .event_policy(EventPolicy::NONE);
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.node = self.node.on_click(handler);
        self
    }

    pub fn on_click_handler(mut self, handler: UiEventHandler) -> Self {
        self.node = self.node.on_click_handler(handler);
        self
    }

    pub fn on_click_async<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(UiAsyncContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.node = self.node.on_click_handler(async_handler(handler));
        self
    }

    pub fn on_click_capture<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.node = self.node.on_click_capture(handler);
        self
    }

    pub fn on_event_handler(
        mut self,
        kind: UiEventKind,
        capture: bool,
        handler: UiInputEventHandler,
    ) -> Self {
        self.node = self.node.on_event_handler(kind, capture, handler);
        self
    }

    pub fn on_event<F>(mut self, kind: UiEventKind, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync + 'static,
    {
        self.node = self.node.on_event(kind, handler);
        self
    }

    pub fn on_event_capture<F>(mut self, kind: UiEventKind, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync + 'static,
    {
        self.node = self.node.on_event_capture(kind, handler);
        self
    }

    pub fn on_click_capture_handler(mut self, handler: UiEventHandler) -> Self {
        self.node = self.node.on_click_capture_handler(handler);
        self
    }

    pub fn wheel_action(mut self, action: UiAction) -> Self {
        self.node = self.node.wheel_action(action);
        self
    }

    pub fn click_action(mut self, action: UiAction) -> Self {
        self.node = self.node.click_action(action);
        self
    }

    pub fn action_target(mut self, target: UiId) -> Self {
        self.node = self.node.action_target(target);
        self
    }

    pub fn on_action<F>(mut self, id: impl Into<super::ActionId>, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiAction) + Send + Sync + 'static,
    {
        self.node = self.node.on_action(id, handler);
        self
    }

    pub fn event_policy(mut self, policy: EventPolicy) -> Self {
        self.node = self.node.event_policy(policy);
        self
    }

    pub fn auto_focus(mut self) -> Self {
        self.node = self.node.auto_focus(true);
        self
    }

    pub fn focus_scope(mut self) -> Self {
        self.node = self.node.focus_scope(true);
        self
    }

    pub fn animation(mut self, binding: AnimationBinding) -> Self {
        self.node = self.node.animation(binding);
        self
    }

    pub fn animation_target(mut self, property: super::AnimProperty, active: bool) -> Self {
        self.node = self.node.animation_target(property, active);
        self
    }

    pub fn animation_outset(mut self, x: f32, y: f32) -> Self {
        self.node = self.node.animation_outset(x, y);
        self
    }

    pub fn layout(mut self, layout: LayoutSpec) -> Self {
        self.node = self.node.layout(layout);
        self
    }

    pub fn style(mut self, style: VisualStyle) -> Self {
        self.node = self.node.style(style);
        self
    }

    pub fn path_content(mut self, path: UiPath, style: PathStyle) -> Self {
        self.node = self.node.path(path, style);
        self
    }

    pub fn text_content(mut self, text: impl Into<Cow<'static, str>>, style: TextStyle) -> Self {
        self.node = self.node.text(text, style);
        self
    }

    pub fn image_source(mut self, source: impl Into<UiImageSource>, fit: ImageFit) -> Self {
        self.node = self.node.image(source, fit);
        self
    }

    pub fn image_request(mut self, request: impl Into<ImageRequest>, fit: ImageFit) -> Self {
        self.node = self.node.image_request(request, fit);
        self
    }

    pub fn icon_key(mut self, key: &'static str) -> Self {
        self.node = self.node.icon(key);
        self
    }

    pub fn icon_style(mut self, style: IconStyle) -> Self {
        self.node = self.node.icon_style(style);
        self
    }

    pub fn glow_effect(mut self, color: Color, alpha: u8) -> Self {
        self.node = self.node.glow(color, alpha);
        self
    }

    pub fn overlay_style(mut self, style: OverlayStyle) -> Self {
        self.node = self.node.overlay(style);
        self
    }

    pub fn backdrop_blur_style(mut self, style: BackdropBlurStyle) -> Self {
        self.node = self.node.backdrop_blur(style);
        self
    }

    pub fn custom_style(mut self, style: CustomPaintStyle) -> Self {
        self.node = self.node.custom_style(style);
        self
    }

    pub fn static_layer_spec(mut self, spec: StaticLayerSpec) -> Self {
        self.node = self.node.static_layer(spec);
        self
    }

    pub fn compositing_layer_spec(mut self, spec: CompositingLayerSpec) -> Self {
        self.node = self.node.compositing_layer(spec);
        self
    }

    /// Shadows this element and its subtree as one alpha silhouette, without changing layout.
    pub fn shadow(mut self, style: super::ShadowStyle) -> Self {
        self.node = self.node.shadow(style);
        self
    }

    pub fn scroll_raster_spec(mut self, spec: ScrollRasterSpec) -> Self {
        self.node = self.node.scroll_raster(spec);
        self
    }

    pub fn clip_content(mut self, rect: UiRect, offset_x: f32, offset_y: f32) -> Self {
        self.node = self.node.clip(rect, offset_x, offset_y);
        self
    }

    pub fn hit_rect(mut self, rect: UiRect) -> Self {
        self.node = self.node.hit_rect(rect);
        self
    }

    pub fn paint_bounds(mut self, rect: UiRect) -> Self {
        self.node = self.node.paint_bounds(rect);
        self
    }

    pub fn ime_cursor_rect(mut self, rect: UiRect) -> Self {
        self.node = self.node.ime_cursor_rect(rect);
        self
    }

    pub fn render_phase(mut self, phase: RenderPhase) -> Self {
        self.node = self.node.render_phase(phase);
        self
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.node = self.node.translate(x, y);
        self.children = Arc::new(
            self.children
                .iter()
                .cloned()
                .map(|child| child.translate(x, y))
                .collect(),
        );
        self
    }

    pub(crate) fn claim_component_owner(mut self, owner: ComponentId) -> Self {
        if self.node.component_owner.is_none() {
            self.node = self.node.component_owner(owner);
        }
        self.children = Arc::new(
            self.children
                .iter()
                .cloned()
                .map(|child| child.claim_component_owner(owner))
                .collect(),
        );
        self
    }

    pub(crate) fn component_boundary(mut self, id: ComponentId) -> Self {
        if self.component_boundary.is_none() {
            self.component_boundary = Some(ComponentBoundary {
                id,
                retain_children: false,
            });
        }
        self
    }

    pub(crate) fn into_retained_boundary(mut self, id: ComponentId) -> Self {
        self.component_boundary = Some(ComponentBoundary {
            id,
            retain_children: true,
        });
        self
    }

    pub(crate) fn into_parts(self) -> (UiNode, Arc<Vec<UiElement>>, Option<ComponentBoundary>) {
        (self.node, self.children, self.component_boundary)
    }

    pub fn mount(self, builder: &mut HostTreeBuilder) -> UiId {
        builder.mount_element(self)
    }
}
