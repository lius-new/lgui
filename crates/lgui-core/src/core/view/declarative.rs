use std::{any::type_name, borrow::Cow, cell::Cell, future::Future, panic::Location, sync::Arc};

use super::{
    async_handler, AnimProperty, AnimationBinding, BlurStyle, Color, ComponentId,
    CompositingLayerAnimation, CompositingLayerSpec, EventPolicy, InteractionRole, KeyboardEvent,
    LayoutSpec, OverlayStyle, PathStyle, PointerData, RenderCx, RenderPhase, Semantics, Size,
    TextStyle, UiAsyncContext, UiElement, UiEventContext, UiEventHandler, UiEventKind,
    UiEventPayload, UiId, UiInputEventBinding, UiInputEventHandler, UiPath, UiRect,
    UiRenderContext, UiScope, VisualStyle, WheelDelta,
};

mod content;
mod element;
mod events;
mod primitives;

pub use element::{
    DeclarativeView, Element, ElementKey, ElementRenderCx, Fragment, IntoElementContent,
};
pub use events::IntoClickHandler;
pub use primitives::{
    animated_compositing_layer, backdrop_blur, backdrop_blur_path, clip, clip_path, component,
    compositing_layer, content_blur, content_text, context_provider, ellipse, fragment, glow,
    group, line, overlay, path, precompiled, text,
};

#[cfg(test)]
#[path = "declarative/declarative_test.rs"]
mod tests;
