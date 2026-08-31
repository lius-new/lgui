use std::{any::type_name, borrow::Cow, cell::Cell, future::Future, panic::Location, sync::Arc};

use super::reactor::RenderCx;
use super::{
    async_handler, AnimProperty, AnimationBinding, Color, ComponentId, CompositingLayerAnimation,
    CompositingLayerSpec, EventPolicy, InteractionRole, KeyboardEvent, LayoutSpec, OverlayStyle,
    PathStyle, PointerData, RenderPhase, Semantics, Size, TextStyle, UiAsyncContext, UiElement,
    UiEventContext, UiEventHandler, UiEventKind, UiEventPayload, UiId, UiInputEventBinding,
    UiInputEventHandler, UiPath, UiRect, UiRenderContext, UiScope, VisualStyle, WheelDelta,
};

#[path = "declarative/content.rs"]
mod content;
#[path = "declarative/element.rs"]
mod declarative_element;
#[path = "declarative/events.rs"]
mod events;
#[path = "declarative/primitives.rs"]
mod primitives;

pub use declarative_element::{
    DeclarativeView, Element, ElementKey, ElementRenderCx, Fragment, IntoElementContent,
};
pub use events::IntoClickHandler;
pub use primitives::{
    animated_compositing_layer, clip, clip_path, component, compositing_layer, content_text,
    context_provider, ellipse, fragment, glow, group, line, overlay, path, precompiled, text,
};

#[cfg(test)]
#[path = "declarative/tests.rs"]
mod tests;
