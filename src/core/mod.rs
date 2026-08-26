#![allow(dead_code, unused_imports)]

mod action;
mod animation;
mod builder;
mod component_state;
mod component_tree;
mod context;
mod context_value;
mod declarative;
mod dirty;
mod effect;
mod element;
mod event;
mod event_context;
mod geometry;
mod handler;
mod hook_state;
mod id;
mod layout;
mod node;
mod observable;
mod reactor;
mod render;
mod router;
mod runtime;
mod scope;
mod static_layer;
mod style;
mod task;
mod tree;

pub use action::{ActionId, UiAction, POINTER_DOWN_ACTION, POINTER_DRAG_ACTION, POINTER_UP_ACTION};
pub use animation::{
    AnimProperty, AnimatedValue, AnimationBinding, AnimationRegistry, AnimationSnapshot,
    AnimationTiming,
};
pub use builder::{HostProjectionMetrics, HostTreeBuilder, RootComponent};
pub use component_state::{ComponentActionOutcome, ComponentState, ComponentStateStore};
pub use component_tree::{
    ComponentId, ComponentRuntimeMetrics, ComponentTree, HookId, HookSlotKind,
};
pub use context::UiRenderContext;
pub use context_value::{ContextProviderGuard, ContextRegistry};
pub use declarative::{
    clip, clip_path, component, content_text, context_provider, ellipse, fragment, glow, group,
    line, overlay, path, precompiled, text, DeclarativeView, Element, ElementKey, ElementRenderCx,
    Fragment, IntoClickHandler, IntoElementContent,
};
pub use dirty::{DirtySet, DirtyTracker};
pub use effect::{EffectRegistry, IntoEffectCleanup, UiEffect};
pub use element::{UiComponent, UiElement};
pub use event::{
    apply_events_to_animations, InputEvent, InteractionFlags, KeyCode, KeyModifiers, PointerButton,
    UiEvent, UiEventDispatcher, UiInteractionState,
};
pub use event_context::{UiAsyncContext, UiEventContext, UiEventFlags};
pub use geometry::{EdgeInsets, Point, Size, UiRect, UiScale};
pub use handler::{
    IntoUiHandler, UiActionBinding, UiActionEvent, UiActionHandler, UiEventHandler, UiEventKind,
    UiEventPayload, UiHandlerEvent, UiInputEventBinding, UiInputEventHandler,
};
pub use hook_state::{HookStateStore, UiUpdateQueue, UiWake};
pub use id::{UiId, UiIdPath};
pub use layout::{
    apply_layout, apply_layout_tree, Align, Axis, LayoutCommitMetrics, LayoutInvalidation,
    LayoutRuntime, LayoutSpec,
};
pub use node::{EventPolicy, InteractionRole, UiImageSource, UiNode, UiNodeKind};
pub use observable::{Observable, ObservableListener};
pub use reactor::{RenderCx, StateSetter, UiFocusHandle};
pub use render::{
    commands_for_phase, compile_scene, compile_scene_root, scene_root_ids,
    scroll_raster_command_snapshot_exists, ImageFit, RenderPhase, Scene, ScenePrimitive,
    ScrollRasterSpec,
};
pub use router::{Navigate, RouterContext};
pub use runtime::{PendingUpdateOutput, RuntimeOutput, UiDefaultAction, UiRuntime};
pub use scope::UiScope;
pub use static_layer::{
    StaticLayerBackground, StaticLayerCachePolicy, StaticLayerSource, StaticLayerSpec,
};
pub use style::{
    BackdropBlurStyle, Color, CustomPaintStyle, IconStyle, OverlayStyle, PathStyle,
    RadialGradientLayer, Stroke, TextAlign, TextStyle, UiPath, UiPathCommand,
    VerticalGradientLayer, VisualStyle,
};
pub use task::{noop_task_spawner, UiTask, UiTaskSpawner};
pub(crate) use tree::ProjectionChanges;
pub use tree::{HitResult, HostTree};
