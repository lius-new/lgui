#[path = "input/action.rs"]
mod action;
mod animation;
#[path = "view/builder.rs"]
mod builder;
#[path = "component/component_state.rs"]
mod component_state;
#[path = "component/component_tree.rs"]
mod component_tree;
#[path = "scene/compositing_layer.rs"]
mod compositing_layer;
#[path = "component/context.rs"]
mod context;
#[path = "component/context_value.rs"]
mod context_value;
#[path = "view/declarative.rs"]
mod declarative;
#[path = "layout/dirty.rs"]
mod dirty;
#[path = "input/dispatch.rs"]
mod dispatch;
#[path = "component/effect.rs"]
mod effect;
#[path = "view/element.rs"]
mod element;
#[path = "input/event.rs"]
mod event;
#[path = "input/event_context.rs"]
mod event_context;
#[path = "foundation/geometry.rs"]
mod geometry;
#[path = "input/handler.rs"]
mod handler;
#[path = "component/hook_state.rs"]
mod hook_state;
#[path = "foundation/id.rs"]
mod id;
#[path = "layout/layout.rs"]
mod layout;
#[path = "view/node.rs"]
mod node;
#[path = "component/observable.rs"]
mod observable;
#[path = "component/reactor.rs"]
mod reactor;
#[path = "scene/render/mod.rs"]
mod render;
#[path = "component/runtime/mod.rs"]
mod runtime;
#[path = "component/scope.rs"]
mod scope;
mod semantics;
#[path = "scene/static_layer.rs"]
mod static_layer;
#[path = "foundation/style.rs"]
mod style;
mod task;
#[path = "view/tree.rs"]
mod tree;

pub use crate::memory::ImageCachePolicy;
#[cfg(feature = "router")]
pub use crate::router::{Back, Navigate, Replace, RouterContext};
pub use action::{ActionId, UiAction, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION, POINTER_UP_ACTION};
pub use animation::{
    AnimProperty, AnimatedValue, AnimationBinding, AnimationRegistry, AnimationSnapshot,
    AnimationTiming,
};
pub use builder::{HostProjectionMetrics, HostTreeBuilder, RootComponent};
pub use component_state::{
    ComponentActionOutcome, ComponentState, ComponentStateStore, CompositingLayerAnimation,
};
pub use component_tree::{
    ComponentId, ComponentRuntimeMetrics, ComponentTree, HookId, HookSlotKind,
};
pub use compositing_layer::{CompositingLayerBackground, CompositingLayerSpec, LayerTransform};
pub use context::UiRenderContext;
pub(crate) use context_value::stage_current_listener;
pub use context_value::{try_use_context, use_context, ContextProviderGuard, ContextRegistry};
pub use declarative::{
    animated_compositing_layer, clip, clip_path, component, compositing_layer, content_text,
    context_provider, ellipse, fragment, glow, group, line, overlay, path, precompiled, text,
    DeclarativeView, Element, ElementKey, ElementRenderCx, Fragment, IntoClickHandler,
    IntoElementContent,
};
pub use dirty::{DirtySet, DirtyTracker};
pub use dispatch::{dispatch_event_handlers, dispatch_hit_handlers, dispatch_runtime_output};
pub use effect::{EffectRegistry, IntoEffectCleanup, UiEffect};
pub use element::{UiComponent, UiElement};
pub use event::{
    apply_events_to_animations, ImeEvent, InputEvent, InteractionFlags, KeyLocation, KeyModifiers,
    KeyState, KeyboardEvent, LogicalKey, NamedKey, PhysicalKey, PlatformEvent, PlatformTheme,
    PointerButton, PointerData, PointerId, PointerKind, SemanticInput, TouchPhase, UiEvent,
    UiEventDispatcher, UiInteractionState, WheelDelta, WheelUnit, WindowStateEvent,
};
pub use event_context::{UiAsyncContext, UiEventContext, UiEventFlags};
pub use geometry::{
    EdgeInsets, PhysicalPoint, PhysicalRect, PhysicalSize, Point, Size, UiRect, UiScale,
};
pub use handler::{
    async_handler, async_handler_with, IntoUiHandler, UiActionBinding, UiActionEvent,
    UiActionHandler, UiEventHandler, UiEventKind, UiEventPayload, UiHandlerEvent,
    UiInputEventBinding, UiInputEventHandler, UiValueEventHandler,
};
pub use hook_state::{HookStateStore, UiUpdateQueue, UiWake};
pub use id::{UiId, UiIdPath};
pub use layout::{
    apply_layout, apply_layout_tree, Align, Axis, LayoutCommitMetrics, LayoutInvalidation,
    LayoutRuntime, LayoutSpec,
};
pub use node::{
    EventPolicy, ImageDecodePolicy, ImageRequest, InteractionRole, UiImageSource, UiNode,
    UiNodeKind,
};
pub use observable::{Observable, ObservableListener};
pub use reactor::{RenderCx, State, StateSetter, UiFocusHandle};
#[cfg(any(
    test,
    feature = "backend-winit",
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    all(feature = "backend-win32", feature = "renderer-skia")
))]
pub(crate) use render::estimate_scene_commands_bytes;
pub(crate) use render::patch_compositing_layer_spec;
#[cfg(all(target_os = "windows", feature = "renderer-d2d"))]
pub(crate) use render::translate_scene_primitive_for_backend;
pub use render::{
    commands_for_phase, compile_scene, compile_scene_root, compositing_layer_damage,
    scene_root_ids, scroll_raster_command_snapshot_exists, ImageFit, RenderPhase, Scene,
    ScenePrimitive, ScenePrimitiveKind, ScrollRasterSpec,
};
pub(crate) use render::{
    scroll_raster_command_cache_usage, set_scroll_raster_command_cache_budget,
    trim_scroll_raster_command_cache,
};
pub use runtime::{PendingUpdateOutput, RuntimeOutput, UiDefaultAction, UiRuntime};
pub use scope::UiScope;
pub use semantics::{
    SemanticAction, SemanticNode, SemanticRelationships, SemanticRole, SemanticState, SemanticText,
    SemanticUpdate, Semantics,
};
pub use static_layer::{
    RasterCachePolicy, StaticLayerBackground, StaticLayerSource, StaticLayerSpec,
};
pub use style::{
    BackdropBlurStyle, Color, CustomPaintStyle, IconStyle, OverlayStyle, PathStyle,
    RadialGradientLayer, Stroke, TextAlign, TextStyle, UiPath, UiPathCommand,
    VerticalGradientLayer, VisualStyle,
};
#[cfg(feature = "tokio")]
pub use task::TokioExecutor;
pub use task::{noop_task_spawner, UiExecutor, UiTask, UiTaskSpawner};
pub(crate) use tree::ProjectionChanges;
pub use tree::{HitResult, HostTree};
