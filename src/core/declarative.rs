use std::{any::type_name, borrow::Cow, cell::Cell, panic::Location, sync::Arc};

use super::reactor::RenderCx;
use super::{
    AnimProperty, AnimationBinding, Color, ComponentId, EventPolicy, InteractionRole, LayoutSpec,
    RenderPhase, Size, TextStyle, UiElement, UiEventContext, UiEventHandler, UiEventKind,
    UiEventPayload, UiId, UiInputEventBinding, UiInputEventHandler, UiPath, UiRect,
    UiRenderContext, UiScope, VisualStyle,
};

// Declarative core shell only: this layer owns tree identity and composition,
// but must not depend on concrete UI components. Component-specific adapters
// belong beside the component they wrap during the migration.
pub trait DeclarativeView {
    fn compile(
        self,
        scope: &UiScope,
        context: &UiRenderContext,
        component_id: ComponentId,
        force_components: bool,
    ) -> UiElement;
}

pub trait IntoElementContent {
    fn append_to(self, children: &mut Vec<Element>);
}

pub trait IntoClickHandler<Arguments> {
    fn into_click_handler(self) -> UiEventHandler;
}

pub enum NoClickArguments {}
pub enum ClickEventArgument {}

impl<F> IntoClickHandler<NoClickArguments> for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn into_click_handler(self) -> UiEventHandler {
        Arc::new(move |_| self())
    }
}

impl<F> IntoClickHandler<ClickEventArgument> for F
where
    F: Fn(&mut UiEventContext) + Send + Sync + 'static,
{
    fn into_click_handler(self) -> UiEventHandler {
        Arc::new(self)
    }
}

pub struct Fragment {
    children: Vec<Element>,
}

pub struct Element {
    key: ElementKey,
    render: Box<dyn FnOnce(ElementRenderCx<'_, '_, '_>) -> UiElement>,
    interaction: Option<InteractionRole>,
    click_capture_handler: Option<UiEventHandler>,
    click_handler: Option<UiEventHandler>,
    input_event_handlers: Vec<UiInputEventBinding>,
    event_policy: Option<EventPolicy>,
    phase: Option<RenderPhase>,
    animations: Vec<AnimationBinding>,
    animation_targets: Vec<(AnimProperty, bool)>,
    paint_bounds: Option<UiRect>,
    focus_scope: bool,
    defer_children: bool,
    children: Vec<Element>,
}

#[derive(Clone, Copy)]
pub struct ElementKey {
    file: &'static str,
    line: u32,
    column: u32,
    explicit: Option<u64>,
}

pub struct ElementRenderCx<'a, 'ctx, 'scope> {
    pub id: UiId,
    pub scope: &'scope UiScope,
    pub context: &'ctx UiRenderContext<'a>,
    pub children: Vec<UiElement>,
    component_id: ComponentId,
    force_components: bool,
    deferred_children: Option<Vec<Element>>,
    id_counter: Cell<u32>,
}

impl ElementRenderCx<'_, '_, '_> {
    /// Returns a unique auto-generated child ID within the current scope.
    /// Use this instead of manually calling `cx.scope.id(format!(...))`.
    pub fn auto_id(&self) -> UiId {
        let n = self.id_counter.get();
        self.id_counter.set(n + 1);
        self.scope.id(format!("{}._{}", self.id.as_str(), n))
    }

    pub fn use_context<T>(&self) -> T
    where
        T: Clone + 'static,
    {
        self.try_use_context::<T>().unwrap_or_else(|| {
            panic!(
                "missing context value `{}` for element `{}`",
                type_name::<T>(),
                self.id.as_str()
            )
        })
    }

    pub fn try_use_context<T>(&self) -> Option<T>
    where
        T: Clone + 'static,
    {
        self.context.contexts().read(self.component_id)
    }

    #[doc(hidden)]
    pub fn component_owner(&self) -> (ComponentId, bool) {
        (self.component_id, self.force_components)
    }
}

impl Element {
    #[track_caller]
    pub fn new(render: impl FnOnce(ElementRenderCx<'_, '_, '_>) -> UiElement + 'static) -> Self {
        Self::with_key(ElementKey::caller(), render)
    }

    pub fn with_key(
        key: ElementKey,
        render: impl FnOnce(ElementRenderCx<'_, '_, '_>) -> UiElement + 'static,
    ) -> Self {
        Self {
            key,
            render: Box::new(render),
            interaction: None,
            click_capture_handler: None,
            click_handler: None,
            input_event_handlers: Vec::new(),
            event_policy: None,
            phase: None,
            animations: Vec::new(),
            animation_targets: Vec::new(),
            paint_bounds: None,
            focus_scope: false,
            defer_children: false,
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn content(mut self, content: impl IntoElementContent) -> Self {
        content.append_to(&mut self.children);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Element>>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }

    pub fn key(mut self, key: impl AsRef<str>) -> Self {
        self.key.explicit = Some(stable_hash(key.as_ref()));
        self
    }

    #[doc(hidden)]
    pub fn source_key(mut self, key: ElementKey) -> Self {
        self.key = key;
        self
    }

    pub fn interaction(mut self, interaction: InteractionRole) -> Self {
        self.interaction = Some(interaction);
        self
    }

    /// Marks this element as a native window drag region.
    ///
    /// Interactive descendants are excluded automatically by hit testing.
    pub fn window_drag_region(mut self) -> Self {
        self.interaction = Some(InteractionRole::WindowDragRegion);
        self.event_policy = Some(EventPolicy::NONE);
        self
    }

    pub fn on_click_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_handler = Some(handler);
        self
    }

    pub fn on_click_capture_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_capture_handler = Some(handler);
        self
    }

    pub fn event_policy(mut self, policy: EventPolicy) -> Self {
        self.event_policy = Some(policy);
        self
    }

    pub fn on_click<F, Arguments>(self, handler: F) -> Self
    where
        F: IntoClickHandler<Arguments>,
    {
        self.on_click_handler(handler.into_click_handler())
    }

    pub fn on_click_event<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_click_handler(Arc::new(handler))
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

    pub fn on_pointer_down<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, super::Point) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerDown, move |cx, payload| {
            if let UiEventPayload::PointerDown { point } = payload {
                handler(cx, *point);
            }
        })
    }

    pub fn on_pointer_move<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, super::Point) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerMove, move |cx, payload| {
            if let UiEventPayload::PointerMove { point } = payload {
                handler(cx, *point);
            }
        })
    }

    pub fn on_pointer_up<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, super::Point) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerUp, move |cx, payload| {
            if let UiEventPayload::PointerUp { point } = payload {
                handler(cx, *point);
            }
        })
    }

    pub fn on_wheel<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, i32) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Wheel, move |cx, payload| {
            if let UiEventPayload::Wheel { delta_y } = payload {
                handler(cx, *delta_y);
            }
        })
    }

    pub fn on_key_down<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, super::KeyCode, super::KeyModifiers) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::KeyDown, move |cx, payload| {
            if let UiEventPayload::KeyDown { key, modifiers } = payload {
                handler(cx, *key, *modifiers);
            }
        })
    }

    pub fn on_input<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &str) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Input, move |cx, payload| {
            if let UiEventPayload::Input { text } = payload {
                handler(cx, text);
            }
        })
    }

    pub fn on_composition_start<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::CompositionStart, move |cx, _| handler(cx))
    }

    pub fn on_composition_update<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &str) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::CompositionUpdate, move |cx, payload| {
            if let UiEventPayload::CompositionUpdate { text } = payload {
                handler(cx, text);
            }
        })
    }

    pub fn on_composition_end<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::CompositionEnd, move |cx, _| handler(cx))
    }

    pub fn on_focus<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Focus, move |cx, _| handler(cx))
    }

    pub fn on_blur<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Blur, move |cx, _| handler(cx))
    }

    pub fn on_change<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, Option<&str>) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Change, move |cx, payload| {
            if let UiEventPayload::Change { value } = payload {
                handler(cx, value.as_deref());
            }
        })
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = Some(phase);
        self
    }

    pub fn animation(mut self, binding: AnimationBinding) -> Self {
        self.animations.push(binding);
        self
    }

    pub fn animation_target(mut self, property: AnimProperty, active: bool) -> Self {
        self.animation_targets.push((property, active));
        self
    }

    pub fn paint_bounds(mut self, rect: UiRect) -> Self {
        self.paint_bounds = Some(rect);
        self
    }

    pub fn focus_scope(mut self) -> Self {
        self.focus_scope = true;
        self
    }

    pub fn defer_children_compile(mut self) -> Self {
        self.defer_children = true;
        self
    }

    fn compile_internal(
        self,
        scope: &UiScope,
        context: &UiRenderContext,
        component_id: ComponentId,
        force_components: bool,
    ) -> UiElement {
        let _current_context = context.contexts().enter_current(component_id);
        let (children, deferred_children) = if self.defer_children {
            (Vec::new(), Some(self.children))
        } else {
            (
                compile_children(
                    scope,
                    context,
                    component_id,
                    force_components,
                    self.children,
                ),
                None,
            )
        };
        let mut element = (self.render)(ElementRenderCx {
            id: scope.node_id(),
            scope,
            context,
            children,
            component_id,
            force_components,
            deferred_children,
            id_counter: Cell::new(0),
        });
        if let Some(interaction) = self.interaction {
            element = element.interaction(interaction);
        }
        if let Some(handler) = self.click_capture_handler {
            element = element.on_click_capture_handler(handler);
        }
        if let Some(handler) = self.click_handler {
            element = element.on_click_handler(handler);
        }
        for binding in self.input_event_handlers {
            element = element.on_event_handler(binding.kind, binding.capture, binding.handler);
        }
        if let Some(policy) = self.event_policy {
            element = element.event_policy(policy);
        }
        if let Some(phase) = self.phase {
            element = element.render_phase(phase);
        }
        for animation in self.animations {
            element = element.animation(animation);
        }
        for (property, active) in self.animation_targets {
            element = element.animation_target(property, active);
        }
        if let Some(bounds) = self.paint_bounds {
            element = element.paint_bounds(bounds);
        }
        if self.focus_scope {
            element = element.focus_scope();
        }
        element
    }
}

impl ElementRenderCx<'_, '_, '_> {
    pub fn animation_value(&self, property: AnimProperty) -> f32 {
        self.context.animation_value(&self.id, property)
    }

    pub fn compile(&self, element: Element) -> UiElement {
        element.compile_internal(
            self.scope,
            self.context,
            self.component_id,
            self.force_components,
        )
    }

    pub fn compile_deferred_children(&mut self) -> Vec<UiElement> {
        self.deferred_children
            .take()
            .map(|children| {
                compile_children(
                    self.scope,
                    self.context,
                    self.component_id,
                    self.force_components,
                    children,
                )
            })
            .unwrap_or_default()
    }
}

impl DeclarativeView for Element {
    fn compile(
        self,
        scope: &UiScope,
        context: &UiRenderContext,
        component_id: ComponentId,
        force_components: bool,
    ) -> UiElement {
        self.compile_internal(scope, context, component_id, force_components)
    }
}

impl ElementKey {
    #[track_caller]
    pub fn caller() -> Self {
        let location = Location::caller();
        Self {
            file: location.file(),
            line: location.line(),
            column: location.column(),
            explicit: None,
        }
    }

    fn segment(self, index: usize) -> String {
        match self.explicit {
            Some(key) => format!("e.{:016x}.k.{key:016x}", self.hash()),
            None => format!("e.{:016x}.{index}", self.hash()),
        }
    }

    fn hash(self) -> u64 {
        let mut hash = FNV_OFFSET;
        hash = hash_bytes(hash, self.file.as_bytes());
        hash = hash_u32(hash, self.line);
        hash_u32(hash, self.column)
    }
}

pub fn group(rect: UiRect) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::group(cx.id, rect).children(cx.children)
    })
}

pub fn text(
    rect: UiRect,
    value: impl Into<std::borrow::Cow<'static, str>>,
    style: TextStyle,
) -> Element {
    let value = value.into();
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::text(cx.id, rect, value, style))
}

pub fn content_text(value: impl Into<Cow<'static, str>>) -> Element {
    let value = value.into();
    let height = 18;
    let width = (value.chars().count() as i32 * 9).max(1);
    let rect = UiRect::new(0, 0, width, height);
    text(rect, value, TextStyle::new(Color::WHITE, 16, 400))
        .with_layout(LayoutSpec::Fixed(Size::new(width, height)))
}

pub fn fragment(content: impl IntoElementContent) -> Fragment {
    let mut children = Vec::new();
    content.append_to(&mut children);
    Fragment { children }
}

/// Provides a typed context value while its declarative descendants are compiled.
///
/// The provider is itself a retained component boundary. This keeps the provider stack
/// active for lazily compiled child components and gives context dependency tracking a
/// stable owner independent of the rendering backend.
#[track_caller]
pub fn context_provider<T>(value: T, content: impl IntoElementContent + 'static) -> Element
where
    T: Clone + PartialEq + 'static,
{
    let mut children = Vec::new();
    content.append_to(&mut children);
    component(value, move |_cx, value| {
        let value = value.clone();
        Element::new(move |mut cx: ElementRenderCx<'_, '_, '_>| {
            let _provider =
                cx.context
                    .contexts()
                    .provide(cx.component_id, value, cx.context.component_tree());
            let children = cx.compile_deferred_children();
            UiElement::group(cx.id, cx.context.viewport()).children(children)
        })
        .defer_children_compile()
        .children(children)
    })
}

pub fn ellipse(rect: UiRect, style: VisualStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::ellipse(cx.id, rect, style))
}

pub fn glow(rect: UiRect, color: Color, alpha: u8) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::glow(cx.id, rect, color, alpha))
}

pub fn line(rect: UiRect, style: VisualStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::line(cx.id, rect, style))
}

pub fn overlay(rect: UiRect, style: super::OverlayStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::overlay(cx.id, rect, style))
}

pub fn clip(rect: UiRect, offset_x: i32, offset_y: i32) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::clip(cx.id, rect, offset_x, offset_y).children(cx.children)
    })
}

pub fn clip_path(rect: UiRect, path: UiPath) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::clip_path(cx.id, rect, path).children(cx.children)
    })
}

pub fn path(rect: UiRect, path: UiPath, style: super::PathStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::path(cx.id, rect, path, style))
}

pub fn precompiled(element: UiElement) -> Element {
    Element::new(move |_| element)
}

/// Declares a real component boundary whose hooks, children and compiled output are retained.
/// Unchanged props and clean dependencies reuse the committed output without executing `render`.
#[track_caller]
pub fn component<P, F>(props: P, render: F) -> Element
where
    P: PartialEq + 'static,
    F: for<'a, 'ctx> FnOnce(&mut RenderCx<'a, 'ctx>, &P) -> Element + 'static,
{
    let location = Location::caller();
    let callsite = location_hash(location);
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        let identity = stable_hash(cx.id.as_str());
        let component_id = cx.context.component_tree().keyed_child(
            cx.component_id,
            callsite,
            identity,
            type_name::<F>(),
        );
        if cx.force_components {
            cx.context.component_tree().mark_dirty(component_id);
        }
        if cx.context.component_tree().can_reuse(component_id, &props) {
            cx.context.preserve_component_state_scope(&cx.id);
            return cx
                .context
                .component_tree()
                .reuse_output(component_id)
                .into_retained_boundary(component_id);
        }

        cx.context.contexts().begin_component(component_id);
        let force_children = cx
            .context
            .component_tree()
            .begin_component_execution(component_id);
        let mut render_cx =
            RenderCx::for_component(cx.scope, cx.context, component_id, force_children);
        let _current_context = cx.context.contexts().enter_current(component_id);
        let view = render(&mut render_cx, &props);
        let output = render_cx
            .compile(view)
            .claim_component_owner(component_id)
            .component_boundary(component_id);
        cx.context
            .component_tree()
            .commit_output(component_id, props, output.clone());
        output
    })
}

impl Element {
    fn with_layout(mut self, layout: LayoutSpec) -> Self {
        let render = self.render;
        self.render = Box::new(move |cx| render(cx).layout(layout));
        self
    }
}

impl IntoElementContent for Element {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(self);
    }
}

impl IntoElementContent for Fragment {
    fn append_to(self, children: &mut Vec<Element>) {
        children.extend(self.children);
    }
}

impl<T> IntoElementContent for Option<T>
where
    T: IntoElementContent,
{
    fn append_to(self, children: &mut Vec<Element>) {
        if let Some(content) = self {
            content.append_to(children);
        }
    }
}

impl<T> IntoElementContent for Vec<T>
where
    T: IntoElementContent,
{
    fn append_to(self, children: &mut Vec<Element>) {
        for content in self {
            content.append_to(children);
        }
    }
}

impl<T, const N: usize> IntoElementContent for [T; N]
where
    T: IntoElementContent,
{
    fn append_to(self, children: &mut Vec<Element>) {
        for content in self {
            content.append_to(children);
        }
    }
}

impl IntoElementContent for String {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(content_text(self));
    }
}

impl IntoElementContent for &'static str {
    fn append_to(self, children: &mut Vec<Element>) {
        children.push(content_text(self));
    }
}

macro_rules! impl_numeric_content {
    ($($value:ty),+ $(,)?) => {
        $(
            impl IntoElementContent for $value {
                fn append_to(self, children: &mut Vec<Element>) {
                    children.push(content_text(self.to_string()));
                }
            }
        )+
    };
}

impl_numeric_content!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);

macro_rules! impl_tuple_content {
    ($($type:ident),+ $(,)?) => {
        impl<$($type),+> IntoElementContent for ($($type,)+)
        where
            $($type: IntoElementContent),+
        {
            #[allow(non_snake_case)]
            fn append_to(self, children: &mut Vec<Element>) {
                let ($($type,)+) = self;
                $($type.append_to(children);)+
            }
        }
    };
}

impl_tuple_content!(A);
impl_tuple_content!(A, B);
impl_tuple_content!(A, B, C);
impl_tuple_content!(A, B, C, D);
impl_tuple_content!(A, B, C, D, E);
impl_tuple_content!(A, B, C, D, E, F);
impl_tuple_content!(A, B, C, D, E, F, G);
impl_tuple_content!(A, B, C, D, E, F, G, H);

fn compile_children(
    scope: &UiScope,
    context: &UiRenderContext,
    component_id: ComponentId,
    force_components: bool,
    children: Vec<Element>,
) -> Vec<UiElement> {
    children
        .into_iter()
        .enumerate()
        .map(|(index, child)| {
            let child_scope = UiScope::from_path(scope.path().child(child.key.segment(index)));
            child.compile_internal(&child_scope, context, component_id, force_components)
        })
        .collect()
}

fn hash_u32(hash: u64, value: u32) -> u64 {
    hash_bytes(hash, &value.to_le_bytes())
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn stable_hash(value: &str) -> u64 {
    hash_bytes(FNV_OFFSET, value.as_bytes())
}

fn location_hash(location: &'static Location<'static>) -> u64 {
    let mut hash = FNV_OFFSET;
    hash = hash_bytes(hash, location.file().as_bytes());
    hash = hash_u32(hash, location.line());
    hash_u32(hash, location.column())
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_flattens_tuples_options_vectors_and_fragments() {
        let mut children = Vec::new();
        (
            "first",
            Some(2_u32),
            vec![content_text("third"), content_text("fourth")],
            fragment(("fifth", None::<Element>)),
        )
            .append_to(&mut children);

        assert_eq!(children.len(), 5);
    }

    #[test]
    fn explicit_keys_do_not_depend_on_list_position() {
        let first = content_text("row").key("stable");
        let second = content_text("row").key("stable");

        assert_eq!(first.key.segment(0), second.key.segment(99));
    }
}
