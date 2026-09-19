use super::element::{location_hash, stable_hash};
use super::*;

pub fn group(rect: UiRect) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::group(cx.id, rect).children(cx.children)
    })
}

pub fn compositing_layer(rect: UiRect, spec: CompositingLayerSpec) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::compositing_layer(cx.id, rect, spec).children(cx.children)
    })
}

/// Creates a retained compositing layer whose application-owned animation state updates only
/// composition properties. Its static children are reused between frames.
#[track_caller]
pub fn animated_compositing_layer<T>(
    rect: UiRect,
    configure: impl FnOnce(&mut T) + 'static,
) -> Element
where
    T: CompositingLayerAnimation,
{
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        let (_, spec, wants_frame) = cx
            .context
            .compositing_layer_animation_mut(&cx.id, configure);
        if wants_frame {
            cx.context.hook_updates().request_frame();
        }
        UiElement::compositing_layer(cx.id, rect, spec).children(cx.children)
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
    let rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
    text(rect, value, TextStyle::new(Color::WHITE, 16.0, 400))
        .with_layout(LayoutSpec::Fixed(Size::new(width as f32, height as f32)))
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
            let bounds = children
                .iter()
                .map(ui_element_paint_bounds)
                .reduce(UiRect::union)
                .unwrap_or_else(|| UiRect::new(0.0, 0.0, 0.0, 0.0));
            UiElement::group(cx.id, cx.context.viewport())
                .paint_bounds(bounds)
                .children(children)
        })
        .defer_children_compile()
        .children(children)
    })
}

fn ui_element_paint_bounds(element: &UiElement) -> UiRect {
    element
        .children_ref()
        .iter()
        .map(ui_element_paint_bounds)
        .fold(element.node().paint_bounds, UiRect::union)
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

pub fn overlay(rect: UiRect, style: OverlayStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| UiElement::overlay(cx.id, rect, style))
}

pub fn backdrop_blur(rect: UiRect, style: BlurStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::backdrop_blur(cx.id, rect, style)
    })
}

pub fn backdrop_blur_path(rect: UiRect, path: UiPath, style: BlurStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::backdrop_blur_path(cx.id, rect, path, style)
    })
}

pub fn content_blur(rect: UiRect, style: BlurStyle) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::content_blur(cx.id, rect, style).children(cx.children)
    })
}

pub fn clip(rect: UiRect, offset_x: f32, offset_y: f32) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::clip(cx.id, rect, offset_x, offset_y).children(cx.children)
    })
}

pub fn clip_path(rect: UiRect, path: UiPath) -> Element {
    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
        UiElement::clip_path(cx.id, rect, path).children(cx.children)
    })
}

pub fn path(rect: UiRect, path: UiPath, style: PathStyle) -> Element {
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
            cx.context.preserve_component_state_scope(cx.scope);
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
