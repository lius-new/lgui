use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(feature = "renderer-skia")]
use std::num::NonZeroU32;

use softbuffer::Context as SoftContext;
#[cfg(feature = "renderer-skia")]
use softbuffer::Surface as SoftSurface;
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
use lgui_core::application::{dispatch_tray_action, TrayRegistration};
#[cfg(feature = "diagnostics")]
use lgui_core::diagnostics::{
    DiagnosticPresentMode, DiagnosticsRegistration, FramePresentMetrics, FrameRenderMetrics,
    FrameSample,
};
use lgui_core::{
    application::{
        AppView, ApplicationBackend, ApplicationContext, ApplicationHandle, ApplicationTask,
        GraphicsPreference,
    },
    core::{
        dispatch_runtime_output, ImeEvent, InputEvent, KeyLocation, KeyModifiers, KeyState,
        KeyboardEvent, LogicalKey, PhysicalKey, PhysicalPoint, PhysicalRect, PhysicalSize, Point,
        PointerButton, PointerData, PointerId, PointerKind, TouchPhase, UiRect, UiScale,
        WheelDelta,
    },
    platform::dpi::{ScaleContext, WorkArea, BASE_DPI},
    session::UiSession,
    window::{ClosePolicy, WindowId, WindowMode, WindowOptions, WindowPosition},
};

use lgui_core::backend::{application_root_view, WindowCommand};
use lgui_render_api::{FrameInfo, FrameReason, MemoryPressure};
#[cfg(feature = "renderer-skia")]
use lgui_render_skia::SkiaSoftwareSurface;

#[cfg(feature = "accessibility")]
#[path = "accessibility.rs"]
mod winit_accessibility;
#[cfg(feature = "renderer-skia-gl")]
#[allow(unsafe_code)]
#[path = "surface/gl.rs"]
mod winit_skia_gl;
#[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
#[allow(unsafe_code)]
#[path = "surface/metal.rs"]
mod winit_skia_metal;
#[cfg(all(
    feature = "renderer-skia-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
#[allow(unsafe_code)]
#[path = "surface/vulkan.rs"]
mod winit_skia_vulkan;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod winit_windows;

mod application;
mod event_loop;
mod input;
mod renderer;
mod window;

pub use application::{WinitApplication, WinitApplicationError};
use event_loop::WinitHost;
pub(crate) use event_loop::WinitUserEvent;
use input::*;
#[cfg(any(
    feature = "renderer-skia-gl",
    feature = "renderer-skia-vulkan",
    feature = "renderer-skia-metal"
))]
pub(crate) use renderer::WinitFrameTimings;
use renderer::*;
use window::*;

#[path = "winit_test.rs"]
#[cfg(test)]
mod tests;
