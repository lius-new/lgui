use std::{
    collections::HashMap,
    fmt,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use softbuffer::{Context as SoftContext, Surface as SoftSurface};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize as WinitPhysicalSize},
    event::{
        ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase as WinitTouchPhase,
        WindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy, OwnedDisplayHandle},
    keyboard::{
        Key as WinitKey, KeyLocation as WinitKeyLocation, ModifiersState,
        PhysicalKey as WinitPhysicalKey,
    },
    window::{
        Fullscreen, ImePurpose, ResizeDirection, Window, WindowAttributes,
        WindowId as WinitWindowId, WindowLevel,
    },
};

#[cfg(all(feature = "tray-win32", target_os = "windows"))]
use crate::application::{dispatch_tray_action, TrayRegistration};
#[cfg(feature = "diagnostics")]
use crate::diagnostics::{
    DiagnosticPresentMode, DiagnosticsRegistration, FramePresentMetrics, FrameRenderMetrics,
    FrameSample,
};
use crate::{
    application::{
        application_root_view, AppView, ApplicationBackend, ApplicationContext, ApplicationHandle,
        ApplicationTask, GraphicsPreference,
    },
    core::{
        dispatch_runtime_output, ImeEvent, InputEvent, KeyLocation, KeyModifiers, KeyState,
        KeyboardEvent, LogicalKey, PhysicalKey, PhysicalPoint, PhysicalRect, PhysicalSize, Point,
        PointerButton, PointerData, PointerId, PointerKind, TouchPhase, UiRect, UiScale,
        WheelDelta,
    },
    platform::dpi::{ScaleContext, WorkArea, BASE_DPI},
    renderer::{FrameInfo, FrameReason, MemoryPressure},
    session::UiSession,
    window::{ClosePolicy, WindowCommand, WindowId, WindowMode, WindowOptions, WindowPosition},
};

use super::skia::{SkiaSoftwareSurface, DEFAULT_CACHE_BUDGET};

use super::skia;

#[cfg(feature = "accessibility")]
use super::winit_accessibility;
#[cfg(feature = "renderer-skia-gl")]
use super::winit_skia_gl;
#[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
use super::winit_skia_metal;
#[cfg(all(
    feature = "renderer-skia-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
use super::winit_skia_vulkan;
#[cfg(target_os = "windows")]
use super::winit_windows;

mod application;
mod event_loop;
mod input;
mod renderer;
mod window;

pub use application::{WinitApplication, WinitApplicationError};
use event_loop::WinitHost;
pub(in crate::platform) use event_loop::WinitUserEvent;
use input::*;
#[cfg(any(
    feature = "renderer-skia-gl",
    feature = "renderer-skia-vulkan",
    feature = "renderer-skia-metal"
))]
pub(in crate::platform) use renderer::WinitFrameTimings;
use renderer::*;
use window::*;

#[cfg(test)]
mod tests;
