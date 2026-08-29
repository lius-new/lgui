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

#[cfg(all(feature = "tray", target_os = "windows"))]
use crate::application::TrayRegistration;
#[cfg(feature = "diagnostics")]
use crate::diagnostics::{
    DiagnosticPresentMode, DiagnosticsRegistration, FramePresentMetrics, FrameRenderMetrics,
    FrameSample,
};
#[cfg(all(feature = "notifications", target_os = "windows"))]
use crate::{
    application::NotificationRegistration,
    platform::{NotificationError, NotificationHandle},
};
use crate::{
    application::{
        application_root_view, AppView, ApplicationBackend, ApplicationContext, ApplicationHandle,
        ApplicationTask, ClosePolicy, GraphicsPreference, WindowCommand, WindowId, WindowMode,
        WindowOptions, WindowPosition,
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
};

use super::skia::{SkiaSoftwareSurface, DEFAULT_CACHE_BUDGET};

#[derive(Debug)]
pub struct WinitApplicationError(String);

impl fmt::Display for WinitApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for WinitApplicationError {}

impl From<winit::error::EventLoopError> for WinitApplicationError {
    fn from(error: winit::error::EventLoopError) -> Self {
        Self(error.to_string())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WinitApplication {
    preference: GraphicsPreference,
}

impl WinitApplication {
    pub fn new(preference: GraphicsPreference) -> Self {
        Self { preference }
    }
}

impl ApplicationBackend for WinitApplication {
    type Error = WinitApplicationError;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        if !graphics_preference_supported(self.preference) {
            return Err(WinitApplicationError(format!(
                "the winit backend does not support the explicit {} driver on this target",
                self.preference.as_str()
            )));
        }

        let event_loop = EventLoop::<WinitUserEvent>::with_user_event()
            .build()
            .map_err(|error| WinitApplicationError(error.to_string()))?;
        let proxy = event_loop.create_proxy();
        context.windows().install({
            let proxy = proxy.clone();
            move |command| {
                let _ = proxy.send_event(WinitUserEvent::Window(command));
            }
        });
        let application_handle = ApplicationHandle::new(
            {
                let proxy = proxy.clone();
                move |task| {
                    let _ = proxy.send_event(WinitUserEvent::Task(task));
                }
            },
            {
                let proxy = proxy.clone();
                move || {
                    let _ = proxy.send_event(WinitUserEvent::RequestAllFrames);
                }
            },
        );
        context.resources().provide(application_handle.clone());
        #[cfg(feature = "images")]
        if context
            .try_resource::<crate::assets::ImageCacheHandle>()
            .is_none()
        {
            let loader = context
                .try_resource::<crate::assets::RemoteImageLoaderHandle>()
                .map(|loader| (*loader).clone())
                .unwrap_or_else(crate::assets::http_image_loader);
            let image_wake = application_handle.clone();
            context
                .resources()
                .provide(crate::assets::async_image_cache(
                    loader,
                    move || image_wake.request_frame(),
                    64 * 1024 * 1024,
                ));
        }
        #[cfg(feature = "store")]
        context.stores().set_wake({
            let proxy = proxy.clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::RequestAllFrames);
            })
        });
        #[cfg(all(feature = "notifications", target_os = "windows"))]
        if let Some(registration) = context.try_resource::<NotificationRegistration>() {
            crate::platform::win32::initialize_notification_identity(&registration.identity)
                .map_err(|error| WinitApplicationError(error.to_string()))?;
            let service =
                crate::platform::win32::Win32NotificationService::new(&registration.identity)
                    .map_err(|error| WinitApplicationError(error.to_string()))?;
            context
                .resources()
                .provide(NotificationHandle::new(move |notification| {
                    service
                        .show(&notification.title, &notification.body)
                        .map_err(|error| NotificationError::new(error.to_string()))
                }));
        }
        let font_families = context
            .try_resource::<crate::text::FontFamilies>()
            .map_or(&["Segoe UI"][..], |families| families.0);
        let _font_families = crate::text::install_font_families(font_families);
        let font_assets = context
            .try_resource::<crate::text::FontAssets>()
            .map_or_else(Default::default, |assets| Arc::clone(&assets.0));
        let _font_assets = crate::text::install_font_assets(font_assets);
        let _text_system = crate::text::install_text_system(super::skia::skia_text_system_handle());

        let display = event_loop.owned_display_handle();
        let soft_context =
            Arc::new(SoftContext::new(display).map_err(|error| {
                WinitApplicationError(format!("create software display: {error}"))
            })?);
        let main_id = options.id.clone();
        let mut handler = WinitHost {
            initial: Some((options, view)),
            main_id,
            context,
            proxy,
            soft_context,
            windows: HashMap::new(),
            ids: HashMap::new(),
            scale_preference: None,
            preference: self.preference,
            #[cfg(all(feature = "tray", target_os = "windows"))]
            tray_host: None,
            exit_requested: false,
        };
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}

pub(super) enum WinitUserEvent {
    Task(ApplicationTask),
    Window(WindowCommand),
    Wake(WindowId),
    RequestAllFrames,
    SetVisible(WindowId, bool),
    #[cfg(feature = "accessibility")]
    Accessibility(accesskit_winit::Event),
}

#[cfg(feature = "accessibility")]
impl From<accesskit_winit::Event> for WinitUserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

struct WinitHost {
    initial: Option<(WindowOptions, AppView)>,
    main_id: WindowId,
    context: ApplicationContext,
    proxy: EventLoopProxy<WinitUserEvent>,
    soft_context: Arc<SoftContext<OwnedDisplayHandle>>,
    windows: HashMap<WinitWindowId, WinitWindow>,
    ids: HashMap<WindowId, WinitWindowId>,
    scale_preference: Option<crate::ScalePreference>,
    preference: GraphicsPreference,
    #[cfg(all(feature = "tray", target_os = "windows"))]
    tray_host: Option<crate::platform::win32::Win32TrayHost>,
    exit_requested: bool,
}

struct WinitWindow {
    id: WindowId,
    options: WindowOptions,
    view: AppView,
    context: ApplicationContext,
    window: Arc<Window>,
    renderer: WinitSkiaRenderer,
    soft_context: Arc<SoftContext<OwnedDisplayHandle>>,
    preference: GraphicsPreference,
    session: UiSession,
    scale: UiScale,
    cursor: Option<Point>,
    modifiers: ModifiersState,
    visible: bool,
    owner_suppressed: bool,
    occluded: bool,
    ime_allowed: bool,
    last_frame: Instant,
    next_frame: Option<Instant>,
    full_redraw: bool,
    recovery: RendererRecoveryState,
    #[cfg(feature = "diagnostics")]
    diagnostics: Option<Arc<DiagnosticsRegistration>>,
    #[cfg(feature = "diagnostics")]
    frame_index: u64,
    #[cfg(feature = "accessibility")]
    accessibility: super::winit_accessibility::AccessibilityState,
}

enum WinitSkiaRenderer {
    Software {
        surface: SoftSurface<OwnedDisplayHandle, Arc<Window>>,
        renderer: SkiaSoftwareSurface,
        fallback_reason: Option<&'static str>,
    },
    #[cfg(feature = "renderer-skia-gl")]
    OpenGl(super::winit_skia_gl::WinitOpenGlRenderer),
    #[cfg(all(
        feature = "renderer-skia-vulkan",
        any(target_os = "windows", target_os = "linux")
    ))]
    Vulkan(super::winit_skia_vulkan::WinitVulkanRenderer),
    #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
    Metal(super::winit_skia_metal::WinitMetalRenderer),
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RendererRecoveryState {
    Healthy,
    Recovering { attempt: u8, gpu_failure: bool },
    Fallback { reason: &'static str, attempts: u8 },
    Failed { attempts: u8 },
}

impl RendererRecoveryState {
    fn attempt(self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::Fallback { attempts, .. } => attempts,
            Self::Recovering { attempt, .. } => attempt,
            Self::Failed { attempts } => attempts,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Recovering { .. } => "recovering",
            Self::Fallback { .. } => "fallback",
            Self::Failed { .. } => "failed",
        }
    }
}

struct WinitRenderError {
    stage: crate::renderer::RenderErrorStage,
    operation: &'static str,
    message: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WinitFrameTimings {
    pub acquire_ms: f32,
    pub draw_ms: f32,
    pub flush_ms: f32,
    pub present_ms: f32,
}

impl WinitRenderError {
    fn new(
        stage: crate::renderer::RenderErrorStage,
        operation: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            operation,
            message: message.into(),
        }
    }
}

impl ApplicationHandler<WinitUserEvent> for WinitHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some((options, view)) = self.initial.take() {
            #[cfg(all(feature = "tray", target_os = "windows"))]
            let main_id = options.id.clone();
            if let Err(error) = self.create_window(event_loop, options, view) {
                self.context
                    .report_render_error(crate::application::RenderError::new(
                        WindowId::new("main"),
                        "skia-software",
                        crate::renderer::RenderErrorStage::Create,
                        "create_winit_window",
                        -1,
                        error.to_string(),
                    ));
                self.exit_requested = true;
                event_loop.exit();
                return;
            }
            #[cfg(all(feature = "tray", target_os = "windows"))]
            if let Some(registration) = self.context.try_resource::<TrayRegistration>() {
                let main_window = self
                    .ids
                    .get(&main_id)
                    .and_then(|native| self.windows.get(native))
                    .map(|window| Arc::clone(&window.window));
                if let Some(main_window) = main_window {
                    let proxy = self.proxy.clone();
                    let visible_id = main_id.clone();
                    match crate::platform::win32::winit_hwnd(&main_window)
                        .map_err(WinitApplicationError)
                        .and_then(|hwnd| {
                            crate::platform::win32::Win32TrayHost::spawn(
                                registration,
                                self.context.clone(),
                                hwnd,
                                move |visible| {
                                    let _ = proxy.send_event(WinitUserEvent::SetVisible(
                                        visible_id.clone(),
                                        visible,
                                    ));
                                },
                            )
                            .map_err(|error| WinitApplicationError(error.to_string()))
                        }) {
                        Ok(host) => self.tray_host = Some(host),
                        Err(error) => {
                            self.context
                                .report_render_error(crate::application::RenderError::new(
                                    main_id,
                                    "platform-winit",
                                    crate::renderer::RenderErrorStage::Create,
                                    "create_tray_host",
                                    -1,
                                    error.to_string(),
                                ))
                        }
                    }
                }
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WinitUserEvent) {
        match event {
            WinitUserEvent::Task(task) => task(),
            WinitUserEvent::Window(command) => self.handle_command(event_loop, command),
            WinitUserEvent::Wake(id) => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.apply_pending_updates();
                }
            }
            WinitUserEvent::RequestAllFrames => {
                for window in self.windows.values_mut() {
                    window.session.invalidate_all();
                    window.full_redraw = true;
                    window.window.request_redraw();
                }
            }
            WinitUserEvent::SetVisible(id, visible) => {
                self.set_window_visibility(&id, visible);
            }
            #[cfg(feature = "accessibility")]
            WinitUserEvent::Accessibility(event) => {
                self.handle_accessibility_event(event);
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WinitWindowId,
        event: WindowEvent,
    ) {
        #[cfg(feature = "accessibility")]
        if let Some(window) = self.windows.get_mut(&window_id) {
            window.accessibility.process_event(&window.window, &event);
        }
        if matches!(event, WindowEvent::CloseRequested) {
            self.request_close(event_loop, window_id);
            return;
        }
        if matches!(event, WindowEvent::Destroyed) {
            self.remove_window(event_loop, window_id);
            return;
        }
        let Some(window) = self.windows.get_mut(&window_id) else {
            return;
        };
        window.handle_event(event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit_requested {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        let mut deadline = None;
        for window in self.windows.values_mut() {
            if window.visible && !window.occluded && window.next_frame.is_some_and(|due| due <= now)
            {
                window.advance_animation(now);
            }
            if window.visible && !window.occluded {
                if let Some(next) = window.next_frame {
                    deadline = Some(deadline.map_or(next, |current: Instant| current.min(next)));
                }
            }
        }
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        if self.windows.is_empty() {
            event_loop.exit();
        }
    }
}

impl WinitHost {
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut options: WindowOptions,
        view: AppView,
    ) -> Result<(), WinitApplicationError> {
        if let Some(preference) = self.scale_preference {
            options.scale_preference = preference;
        }
        if let Some(existing) = self.ids.get(&options.id).copied() {
            let _ = existing;
            self.set_window_visibility(&options.id, true);
            return Ok(());
        }

        let scale = initial_scale(event_loop, &options);
        let physical = scale.physical_size(options.size);
        let mut attributes = WindowAttributes::default()
            .with_title(options.title.clone())
            .with_visible(false)
            .with_resizable(options.resizable)
            .with_decorations(options.native_titlebar)
            .with_transparent(options.transparent)
            .with_window_level(if options.topmost {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            })
            .with_inner_size(WinitPhysicalSize::new(
                physical.width.max(1) as u32,
                physical.height.max(1) as u32,
            ));
        if let Some(minimum) = options.minimum_size {
            let minimum = scale.physical_size(minimum);
            attributes = attributes.with_min_inner_size(WinitPhysicalSize::new(
                minimum.width.max(1) as u32,
                minimum.height.max(1) as u32,
            ));
        }
        if let Some(maximum) = options.maximum_size {
            let maximum = scale.physical_size(maximum);
            attributes = attributes.with_max_inner_size(WinitPhysicalSize::new(
                maximum.width.max(1) as u32,
                maximum.height.max(1) as u32,
            ));
        }
        if options.mode == WindowMode::Fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        if let WindowPosition::Absolute { x, y } = options.position {
            attributes = attributes.with_position(PhysicalPosition::new(x, y));
        }
        #[cfg(target_os = "windows")]
        {
            attributes = super::winit_windows::with_corner_radius(
                attributes,
                options.corner_radius,
                options.mode,
            );
        }

        let owner_window = options
            .owner
            .as_ref()
            .and_then(|owner| self.ids.get(owner))
            .and_then(|native| self.windows.get(native))
            .map(|owner| Arc::clone(&owner.window));
        let cursor_position = options
            .owner
            .as_ref()
            .and_then(|owner| self.ids.get(owner))
            .and_then(|native| self.windows.get(native))
            .and_then(WinitWindow::screen_cursor_position)
            .or_else(|| {
                self.windows
                    .values()
                    .find_map(WinitWindow::screen_cursor_position)
            });
        #[cfg(target_os = "windows")]
        if let Some(owner) = owner_window.as_deref() {
            if let Ok(owned) = super::winit_windows::with_owner(attributes.clone(), owner) {
                attributes = owned;
            }
        }
        let native = event_loop
            .create_window(attributes)
            .map_err(|error| WinitApplicationError(format!("create window: {error}")))?;
        position_window(&native, &options, owner_window.as_deref(), cursor_position);
        let window = Arc::new(native);
        #[cfg(feature = "accessibility")]
        let accessibility = super::winit_accessibility::AccessibilityState::new(
            event_loop,
            &window,
            self.proxy.clone(),
        );
        let renderer = create_renderer(
            self.preference,
            &self.soft_context,
            Arc::clone(&window),
            options.transparent,
        )?;
        let mut session = UiSession::new();
        if let Some(executor) = self.context.task_spawner() {
            session.set_task_spawner(executor);
        }
        let id = options.id.clone();
        session.set_wake(Arc::new({
            let proxy = self.proxy.clone();
            let id = id.clone();
            move || {
                let _ = proxy.send_event(WinitUserEvent::Wake(id.clone()));
            }
        }));
        let visible = options.visible;
        let owner_suppressed = options.owner.as_ref().is_some_and(|owner| {
            self.ids
                .get(owner)
                .and_then(|native| self.windows.get(native))
                .is_some_and(|owner| !owner.visible || owner.owner_suppressed)
        });
        let native_id = window.id();
        let recovery = renderer_recovery_state(&renderer);
        self.ids.insert(id.clone(), native_id);
        self.windows.insert(
            native_id,
            WinitWindow {
                id,
                options,
                view,
                context: self.context.clone(),
                window: Arc::clone(&window),
                renderer,
                soft_context: Arc::clone(&self.soft_context),
                preference: self.preference,
                session,
                scale,
                cursor: None,
                modifiers: ModifiersState::empty(),
                visible,
                owner_suppressed,
                occluded: false,
                ime_allowed: false,
                last_frame: Instant::now(),
                next_frame: None,
                full_redraw: true,
                recovery,
                #[cfg(feature = "diagnostics")]
                diagnostics: self.context.try_resource::<DiagnosticsRegistration>(),
                #[cfg(feature = "diagnostics")]
                frame_index: 0,
                #[cfg(feature = "accessibility")]
                accessibility,
            },
        );
        window.set_visible(visible && !owner_suppressed);
        window.request_redraw();
        Ok(())
    }

    fn handle_command(&mut self, event_loop: &ActiveEventLoop, command: WindowCommand) {
        match command {
            WindowCommand::Show { options, view } => {
                self.create_auxiliary_window(event_loop, options, view);
            }
            WindowCommand::Toggle { options, view } => {
                if let Some(native) = self.ids.get(&options.id).copied() {
                    let visible = self
                        .windows
                        .get(&native)
                        .is_some_and(|window| !window.visible);
                    self.set_window_visibility(&options.id, visible);
                } else {
                    self.create_auxiliary_window(event_loop, options, view);
                }
            }
            WindowCommand::Hide(id) => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.visible = false;
                    window.window.set_visible(false);
                    if window.options.background_memory_optimization {
                        window.renderer.trim(MemoryPressure::Critical);
                        window.session.suspend_rendering();
                    }
                }
                self.suppress_owned_windows(&id, true);
            }
            WindowCommand::Close(id) => {
                if let Some(native) = self.ids.get(&id).copied() {
                    self.remove_window(event_loop, native);
                }
            }
            WindowCommand::RequestClose(id) => {
                if let Some(native) = self.ids.get(&id).copied() {
                    self.request_close(event_loop, native);
                }
            }
            WindowCommand::Minimize(id) => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.window.set_minimized(true);
                }
            }
            WindowCommand::SetScalePreference(preference) => {
                self.scale_preference = Some(preference);
                for window in self.windows.values_mut() {
                    window.options.scale_preference = preference;
                    window.update_scale();
                }
            }
            WindowCommand::SetMode { id, mode } => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.options.mode = mode;
                    window.window.set_fullscreen(match mode {
                        WindowMode::Windowed => None,
                        WindowMode::Fullscreen => Some(Fullscreen::Borderless(None)),
                    });
                    #[cfg(target_os = "windows")]
                    super::winit_windows::set_corner_radius(
                        &window.window,
                        window.options.corner_radius,
                        mode,
                    );
                    window.full_redraw = true;
                    window.window.request_redraw();
                }
            }
            WindowCommand::Input { id, input } => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.dispatch_input(input);
                }
            }
            WindowCommand::Exit => {
                self.exit_requested = true;
                event_loop.exit();
            }
        }
    }

    fn create_auxiliary_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        options: WindowOptions,
        view: AppView,
    ) {
        let (options, view) = prepare_auxiliary_window(&self.context, &self.main_id, options, view);
        let id = options.id.clone();
        if let Err(error) = self.create_window(event_loop, options, view) {
            self.context
                .report_render_error(crate::application::RenderError::new(
                    id,
                    "platform-winit",
                    crate::renderer::RenderErrorStage::Create,
                    "create_auxiliary_window",
                    -1,
                    error.to_string(),
                ));
        }
    }

    #[cfg(feature = "accessibility")]
    fn handle_accessibility_event(&mut self, event: accesskit_winit::Event) {
        use accesskit_winit::WindowEvent as AccessibilityEvent;
        let Some(window) = self.windows.get_mut(&event.window_id) else {
            return;
        };
        match event.window_event {
            AccessibilityEvent::InitialTreeRequested => {
                window.accessibility.request_full();
                window.session.invalidate_all();
                window.full_redraw = true;
                window.window.request_redraw();
            }
            AccessibilityEvent::ActionRequested(request) => {
                let Some(target) = window.accessibility.resolve(request.target_node) else {
                    return;
                };
                if let Some(input) = super::winit_accessibility::semantic_input(request, target) {
                    window.dispatch_input(InputEvent::Semantic(input));
                }
            }
            AccessibilityEvent::AccessibilityDeactivated => {}
        }
    }

    fn request_close(&mut self, event_loop: &ActiveEventLoop, native: WinitWindowId) {
        let Some(policy) = self
            .windows
            .get(&native)
            .map(|window| window.options.close_policy)
        else {
            return;
        };
        match policy {
            ClosePolicy::Exit => self.remove_window(event_loop, native),
            ClosePolicy::Hide => {
                let id = {
                    let window = self.windows.get_mut(&native).expect("window exists");
                    window.visible = false;
                    window.window.set_visible(false);
                    window.id.clone()
                };
                self.suppress_owned_windows(&id, true);
            }
            ClosePolicy::Notify => {
                let window = self.windows.get_mut(&native).expect("window exists");
                if let Some(handler) = window.options.close_handler {
                    let mut event =
                        crate::core::UiEventContext::new(window.context.clone(), window.id.clone());
                    handler(&mut event);
                    if event.flags().needs_frame {
                        window.session.invalidate_all();
                        window.window.request_redraw();
                    }
                }
            }
        }
    }

    fn remove_window(&mut self, event_loop: &ActiveEventLoop, native: WinitWindowId) {
        if let Some(window) = self.windows.remove(&native) {
            let owned = self
                .windows
                .iter()
                .filter(|(_, candidate)| candidate.options.owner.as_ref() == Some(&window.id))
                .map(|(native, _)| *native)
                .collect::<Vec<_>>();
            self.ids.remove(&window.id);
            for owned in owned {
                self.remove_window(event_loop, owned);
            }
        }
        if self.windows.is_empty() {
            event_loop.exit();
        }
    }

    fn window_by_id_mut(&mut self, id: &WindowId) -> Option<&mut WinitWindow> {
        let native = self.ids.get(id).copied()?;
        self.windows.get_mut(&native)
    }

    fn set_window_visibility(&mut self, id: &WindowId, visible: bool) {
        if let Some(window) = self.window_by_id_mut(id) {
            window.visible = visible;
            window
                .window
                .set_visible(visible && !window.owner_suppressed);
            if visible && !window.owner_suppressed {
                window.full_redraw = true;
                window.window.request_redraw();
            }
        }
        self.suppress_owned_windows(id, !visible);
    }

    fn suppress_owned_windows(&mut self, owner: &WindowId, suppressed: bool) {
        let children = self
            .windows
            .iter()
            .filter(|(_, window)| window.options.owner.as_ref() == Some(owner))
            .map(|(_, window)| window.id.clone())
            .collect::<Vec<_>>();
        for child in children {
            if let Some(window) = self.window_by_id_mut(&child) {
                window.owner_suppressed = suppressed;
                window.window.set_visible(window.visible && !suppressed);
                if window.visible && !suppressed {
                    window.full_redraw = true;
                    window.window.request_redraw();
                }
            }
            self.suppress_owned_windows(&child, suppressed);
        }
    }
}

fn prepare_auxiliary_window(
    context: &ApplicationContext,
    main_id: &WindowId,
    mut options: WindowOptions,
    view: AppView,
) -> (WindowOptions, AppView) {
    if options.owner.is_none() && options.id != *main_id {
        options.owner = Some(main_id.clone());
    }
    (options, application_root_view(context.clone(), view))
}

impl WinitWindow {
    fn screen_cursor_position(&self) -> Option<PhysicalPosition<i32>> {
        let cursor = self.cursor?;
        let origin = self.window.outer_position().ok()?;
        let point = self.scale.physical_point(cursor);
        Some(PhysicalPosition::new(
            origin.x + point.x,
            origin.y + point.y,
        ))
    }

    fn handle_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(_) => {
                self.full_redraw = true;
                self.session.invalidate_all();
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.update_scale();
                self.dispatch_input(InputEvent::Platform(
                    crate::core::PlatformEvent::ScaleFactorChanged(self.window.scale_factor()),
                ));
            }
            WindowEvent::Focused(focused) => {
                self.dispatch_input(InputEvent::Platform(crate::core::PlatformEvent::Focused(
                    focused,
                )));
                if !focused && self.options.hide_on_deactivate {
                    self.visible = false;
                    self.window.set_visible(false);
                }
            }
            WindowEvent::ThemeChanged(theme) => self.dispatch_input(InputEvent::Platform(
                crate::core::PlatformEvent::ThemeChanged(match theme {
                    winit::window::Theme::Light => crate::core::PlatformTheme::Light,
                    winit::window::Theme::Dark => crate::core::PlatformTheme::Dark,
                }),
            )),
            WindowEvent::Occluded(occluded) => self.occluded = occluded,
            WindowEvent::CursorMoved { position, .. } => {
                let point = self.scale.logical_point(PhysicalPoint::new(
                    position.x.round() as i32,
                    position.y.round() as i32,
                ));
                self.cursor = Some(point);
                self.dispatch_input(InputEvent::PointerMove(PointerData::mouse(point)));
            }
            WindowEvent::CursorEntered { .. } => {
                if let Some(point) = self.cursor {
                    self.dispatch_input(InputEvent::PointerEnter(PointerData::mouse(point)));
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let point = self.cursor.unwrap_or_default();
                self.dispatch_input(InputEvent::PointerLeave(PointerData::mouse(point)));
                self.cursor = None;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pointer = PointerData::mouse(self.cursor.unwrap_or_default());
                let button = pointer_button(button);
                if state == ElementState::Pressed
                    && button == PointerButton::Left
                    && !self.options.native_titlebar
                {
                    let size = self.window.inner_size();
                    let physical = self.scale.physical_point(pointer.point);
                    if self.options.resizable {
                        if let Some(direction) = resize_direction(
                            physical,
                            WinitPhysicalSize::new(size.width, size.height),
                            (6.0 * self.scale.factor()).ceil().max(1.0) as i32,
                        ) {
                            let _ = self.window.drag_resize_window(direction);
                        } else if self
                            .session
                            .tree()
                            .hit_test(pointer.point)
                            .is_some_and(|hit| {
                                hit.interaction == crate::core::InteractionRole::WindowDragRegion
                            })
                        {
                            let _ = self.window.drag_window();
                        }
                    } else if self
                        .session
                        .tree()
                        .hit_test(pointer.point)
                        .is_some_and(|hit| {
                            hit.interaction == crate::core::InteractionRole::WindowDragRegion
                        })
                    {
                        let _ = self.window.drag_window();
                    }
                }
                self.dispatch_input(match state {
                    ElementState::Pressed => InputEvent::PointerDown { pointer, button },
                    ElementState::Released => InputEvent::PointerUp { pointer, button },
                });
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let point = self.cursor.unwrap_or_default();
                let phase = touch_phase(phase);
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta {
                        x,
                        y,
                        unit: crate::core::WheelUnit::Lines,
                        phase,
                    },
                    MouseScrollDelta::PixelDelta(position) => WheelDelta {
                        x: position.x as f32 / self.scale.factor(),
                        y: position.y as f32 / self.scale.factor(),
                        unit: crate::core::WheelUnit::Pixels,
                        phase,
                    },
                };
                self.dispatch_input(InputEvent::Wheel { point, delta });
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                self.dispatch_input(InputEvent::Keyboard(keyboard_event(event, self.modifiers)));
            }
            WindowEvent::Ime(event) => self.dispatch_input(InputEvent::Ime(match event {
                Ime::Enabled => ImeEvent::Enabled,
                Ime::Preedit(text, cursor) => ImeEvent::Preedit {
                    text,
                    cursor: cursor.map(|(start, end)| start..end),
                },
                Ime::Commit(text) => ImeEvent::Commit(text),
                Ime::Disabled => ImeEvent::Disabled,
            })),
            WindowEvent::Touch(touch) => {
                let point = self.scale.logical_point(PhysicalPoint::new(
                    touch.location.x.round() as i32,
                    touch.location.y.round() as i32,
                ));
                self.dispatch_input(InputEvent::Touch {
                    pointer: PointerData {
                        id: PointerId(touch.id),
                        kind: PointerKind::Touch,
                        point,
                        pressure: touch.force.map(|force| {
                            (force.normalized().clamp(0.0, 1.0) * u16::MAX as f64).round() as u16
                        }),
                        primary: touch.id == 0,
                    },
                    phase: touch_phase(touch.phase),
                });
            }
            WindowEvent::HoveredFile(path) => self.dispatch_input(InputEvent::Platform(
                crate::core::PlatformEvent::FileHovered(path),
            )),
            WindowEvent::DroppedFile(path) => self.dispatch_input(InputEvent::Platform(
                crate::core::PlatformEvent::FileDropped(path),
            )),
            WindowEvent::HoveredFileCancelled => self.dispatch_input(InputEvent::Platform(
                crate::core::PlatformEvent::FileHoverCancelled,
            )),
            _ => {}
        }
    }

    fn dispatch_input(&mut self, input: InputEvent) {
        let output = self.session.handle_input(input);
        let mut context_frame = false;
        let output = dispatch_runtime_output(
            output,
            &self.context,
            &self.id,
            |action| self.session.handle_default_action(action),
            |context| context_frame |= context.flags().needs_frame,
        );
        if context_frame {
            self.session.invalidate_all();
            self.full_redraw = true;
        }
        if let Some(bounds) = output.dirty_bounds {
            self.session.invalidations_mut().invalidate_rect(bounds);
        }
        self.sync_ime();
        self.apply_pending_updates();
        if output.animation_changed {
            self.schedule_next_frame();
        }
    }

    fn apply_pending_updates(&mut self) {
        let updates = self.session.apply_pending_updates();
        let has_dirty_ids = !updates.dirty_ids.is_empty();
        if updates.focus_changed {
            self.session.invalidate_all();
            self.full_redraw = true;
        } else if !updates.dirty_ids.is_empty() {
            if let Some(bounds) = self.session.tree().paint_bounds(updates.dirty_ids) {
                self.session.invalidations_mut().invalidate_rect(bounds);
            } else {
                self.session.invalidate_all();
                self.full_redraw = true;
            }
        }
        if self.full_redraw || updates.frame_requested || has_dirty_ids {
            self.window.request_redraw();
        }
    }

    fn sync_ime(&mut self) {
        let focused = self
            .session
            .runtime()
            .interaction_state()
            .focused
            .as_ref()
            .and_then(|id| self.session.tree().node(id))
            .and_then(|node| {
                let role = node.semantics.as_ref()?.role;
                matches!(
                    role,
                    crate::core::SemanticRole::TextInput
                        | crate::core::SemanticRole::PasswordInput
                        | crate::core::SemanticRole::SearchInput
                )
                .then_some((node.ime_cursor_rect.unwrap_or(node.layout_rect), role))
            });
        let allowed = focused.is_some();
        if allowed != self.ime_allowed {
            self.window.set_ime_allowed(allowed);
            self.ime_allowed = allowed;
        }
        let Some((rect, role)) = focused else {
            return;
        };
        self.window.set_ime_purpose(
            (role == crate::core::SemanticRole::PasswordInput)
                .then_some(ImePurpose::Password)
                .unwrap_or(ImePurpose::Normal),
        );
        let point = self
            .scale
            .physical_point(Point::new(rect.left, rect.bottom + 2.0));
        let height = self.scale.physical_length(rect.height().max(1.0));
        self.window.set_ime_cursor_area(
            PhysicalPosition::new(point.x, point.y),
            WinitPhysicalSize::new(1_u32, height.max(1) as u32),
        );
    }

    fn advance_animation(&mut self, now: Instant) {
        self.next_frame = None;
        let elapsed = now.saturating_duration_since(self.last_frame);
        let output = self.session.advance(elapsed.as_secs_f32() * 1000.0);
        self.last_frame = now;
        if let Some(bounds) = output.dirty_bounds {
            self.session.invalidations_mut().invalidate_rect(bounds);
        }
        if output.animation_changed {
            self.window.request_redraw();
        }
        self.schedule_next_frame();
    }

    fn schedule_next_frame(&mut self) {
        self.next_frame = self
            .session
            .runtime()
            .frame_interval_ms()
            .map(|milliseconds| Instant::now() + Duration::from_millis(milliseconds.max(1)));
    }

    fn update_scale(&mut self) {
        let monitor = self.window.current_monitor();
        self.scale = scale_for_monitor(monitor.as_ref(), &self.options);
        self.session.invalidate_all();
        self.full_redraw = true;
        self.window.request_redraw();
    }

    fn render(&mut self) {
        if !self.visible
            || self.owner_suppressed
            || self.occluded
            || matches!(self.recovery, RendererRecoveryState::Failed { .. })
        {
            return;
        }
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let physical = PhysicalSize::new(size.width as i32, size.height as i32);
        let logical = self.scale.logical_size(physical);
        let viewport = UiRect::new(0.0, 0.0, logical.width, logical.height);
        #[cfg(feature = "images")]
        let _image_cache = self
            .context
            .try_resource::<crate::assets::ImageCacheHandle>()
            .map(|cache| crate::assets::install_image_cache((*cache).clone()));
        #[cfg(feature = "diagnostics")]
        let frame_started = Instant::now();
        let commit = self.session.render_view(&self.view, viewport, self.scale);
        #[cfg(feature = "diagnostics")]
        let frame_build_ms = frame_started.elapsed().as_secs_f32() * 1_000.0;
        #[cfg(feature = "accessibility")]
        self.accessibility.publish(
            &commit.semantics,
            self.session.tree(),
            self.session.runtime().interaction_state().focused,
            self.scale,
        );
        let physical_viewport = PhysicalRect::new(0, 0, physical.width, physical.height);
        let damage = if self.full_redraw {
            vec![physical_viewport]
        } else {
            commit
                .damage
                .dirty
                .effective_rects()
                .into_iter()
                .filter_map(|rect| {
                    self.scale
                        .physical_rect_outward(rect)
                        .intersect(physical_viewport)
                })
                .collect::<Vec<_>>()
        };
        if damage.is_empty() && !self.full_redraw {
            self.schedule_next_frame();
            return;
        }
        let scene = commit.scene.project_to_physical(self.scale);
        let frame = FrameInfo::new(
            physical_viewport,
            &damage,
            self.scale,
            if self.full_redraw {
                FrameReason::Resize
            } else {
                FrameReason::SceneChange
            },
            self.full_redraw || commit.damage.dirty.is_full(),
        );
        let resources = self
            .context
            .try_resource::<crate::assets::RenderResources>()
            .map(|resources| (*resources).clone())
            .unwrap_or_default();
        #[cfg(feature = "svg")]
        let icons = self
            .context
            .try_resource::<crate::icons::IconRegistration>()
            .map(|registration| Arc::clone(&registration.0));
        #[cfg(feature = "svg")]
        #[cfg(feature = "diagnostics")]
        let draw_started = Instant::now();
        let result = crate::icons::with_icon_registry(icons, || {
            crate::assets::with_render_resources(resources, || {
                self.renderer.draw_and_present(&scene, &frame, &damage)
            })
        });
        #[cfg(not(feature = "svg"))]
        let result = crate::assets::with_render_resources(resources, || {
            self.renderer.draw_and_present(&scene, &frame, &damage)
        });
        #[cfg(feature = "diagnostics")]
        let draw_present_ms = draw_started.elapsed().as_secs_f32() * 1_000.0;
        let frame_timings = match result {
            Ok(timings) => timings,
            Err(error) => {
                let failed_stage = error.stage;
                let failed_operation = error.operation;
                let failed_message = error.message.clone();
                self.context
                    .report_render_error(crate::application::RenderError::new(
                        self.id.clone(),
                        self.renderer.name(),
                        error.stage,
                        error.operation,
                        -1,
                        error.message,
                    ));
                self.recover_renderer(failed_stage, failed_operation, &failed_message);
                return;
            }
        };
        self.recovery = match self.recovery {
            RendererRecoveryState::Fallback { reason, .. } => RendererRecoveryState::Fallback {
                reason,
                attempts: 0,
            },
            _ => RendererRecoveryState::Healthy,
        };
        #[cfg(feature = "diagnostics")]
        if let Some(diagnostics) = self.diagnostics.clone() {
            self.frame_index = self.frame_index.wrapping_add(1);
            let viewport_pixels = (physical.width.max(1) as u64) * (physical.height.max(1) as u64);
            let dirty_pixels = damage.iter().fold(0_u64, |total, rect| {
                total.saturating_add(rect.width().max(0) as u64 * rect.height().max(0) as u64)
            });
            diagnostics.record(
                FrameSample {
                    frame_index: self.frame_index,
                    recorded_at: Instant::now(),
                    backend: self.renderer.name(),
                    renderer: self.renderer.device_info(),
                    mode: if frame.is_full_redraw() {
                        DiagnosticPresentMode::Full
                    } else {
                        DiagnosticPresentMode::Dirty
                    },
                    frame_build_ms,
                    diff_ms: 0.0,
                    draw_present_ms,
                    total_ms: frame_started.elapsed().as_secs_f32() * 1_000.0,
                    dirty_rect_count: damage.len(),
                    dirty_area_ratio: dirty_pixels as f32 / viewport_pixels as f32,
                    submit_scope: if frame.is_full_redraw() {
                        "full"
                    } else {
                        "dirty"
                    },
                    fallback_reason: self.renderer.fallback_reason(),
                    primary_reason: None,
                    recovery_state: self.recovery.label(),
                    recovery_attempt: self.recovery.attempt(),
                    render: FrameRenderMetrics {
                        build_host_tree_ms: frame_build_ms,
                        render_total_ms: frame_build_ms,
                        node_count: self.session.tree().nodes().len(),
                        command_count: scene.commands().len(),
                        host_visited_nodes: commit.metrics.visited_host_nodes,
                        scene_compiled_nodes: commit.metrics.compiled_scene_nodes,
                        host_mutations: commit.metrics.host_mutations,
                        scene_mutations: commit.metrics.scene_mutations,
                        reused_scene_nodes: commit.metrics.reused_scene_nodes,
                        ..FrameRenderMetrics::default()
                    },
                    present: {
                        let cache = self.renderer.cache_stats();
                        FramePresentMetrics {
                            acquire_ms: frame_timings.acquire_ms,
                            draw_commands_ms: frame_timings.draw_ms,
                            flush_ms: frame_timings.flush_ms,
                            submit_ms: frame_timings.flush_ms,
                            present_ms: frame_timings.present_ms,
                            submitted_pixels: dirty_pixels,
                            fallback_count: usize::from(self.renderer.fallback_reason().is_some()),
                            cache_budget_bytes: cache.budget_bytes,
                            cache_resident_bytes: cache.resident_bytes,
                            cache_entries: cache.entries,
                            cache_hits: cache.hits,
                            cache_misses: cache.misses,
                            cache_evictions: cache.evictions,
                            text_cache_resident_bytes: cache.text_resident_bytes,
                            text_cache_entries: cache.text_entries,
                            text_cache_hits: cache.text_hits,
                            text_cache_misses: cache.text_misses,
                            text_cache_evictions: cache.text_evictions,
                            largest_cache_entry_bytes: cache.largest_entry_bytes,
                            largest_text_cache_entry_bytes: cache.largest_text_entry_bytes,
                            ..FramePresentMetrics::default()
                        }
                    },
                },
                self.session.tree(),
                viewport,
            );
        }
        self.full_redraw = false;
        self.sync_ime();
        self.session.runtime().run_effects();
        self.schedule_next_frame();
    }

    fn recover_renderer(
        &mut self,
        stage: crate::renderer::RenderErrorStage,
        operation: &'static str,
        message: &str,
    ) {
        const MAX_RECOVERY_ATTEMPTS: u8 = 3;
        let attempt = self.recovery.attempt().saturating_add(1);
        let was_fallback = matches!(self.recovery, RendererRecoveryState::Fallback { .. });
        let gpu_failure = match self.recovery {
            RendererRecoveryState::Recovering { gpu_failure, .. } => {
                gpu_failure || self.renderer.is_gpu()
            }
            RendererRecoveryState::Fallback { .. } => true,
            _ => self.renderer.is_gpu(),
        };
        if attempt > MAX_RECOVERY_ATTEMPTS {
            self.recovery = RendererRecoveryState::Failed {
                attempts: MAX_RECOVERY_ATTEMPTS,
            };
            return;
        }

        self.recovery = RendererRecoveryState::Recovering {
            attempt,
            gpu_failure,
        };
        self.renderer.trim(MemoryPressure::Critical);
        let (recovery_preference, fallback_to_software) =
            recovery_preference(self.preference, was_fallback, gpu_failure, attempt);

        let previous = std::mem::replace(&mut self.renderer, WinitSkiaRenderer::Unavailable);
        drop(previous);
        match create_renderer(
            recovery_preference,
            &self.soft_context,
            Arc::clone(&self.window),
            self.options.transparent,
        ) {
            Ok(mut renderer) => {
                if fallback_to_software {
                    renderer.set_fallback_reason("gpu-runtime-failed");
                }
                let fallback_reason = renderer.fallback_reason();
                self.renderer = renderer;
                if let Some(reason) = fallback_reason {
                    self.recovery = RendererRecoveryState::Fallback {
                        reason,
                        attempts: attempt,
                    };
                }
                self.session.invalidate_all();
                self.full_redraw = true;
                self.window.request_redraw();
            }
            Err(recovery_error) => {
                self.context.report_render_error(crate::application::RenderError::new(
                    self.id.clone(),
                    "skia-recovery",
                    crate::renderer::RenderErrorStage::Create,
                    "recreate_renderer",
                    -1,
                    format!(
                        "attempt {attempt} after {stage:?}/{operation}: {message}; recreation failed: {recovery_error}"
                    ),
                ));
                if attempt == MAX_RECOVERY_ATTEMPTS {
                    self.recovery = RendererRecoveryState::Failed { attempts: attempt };
                } else {
                    self.window.request_redraw();
                }
            }
        }
    }
}

impl WinitSkiaRenderer {
    fn name(&self) -> &'static str {
        match self {
            Self::Software { .. } => "skia-software",
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(_) => "skia-opengl",
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(_) => "skia-vulkan",
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(_) => "skia-metal",
            Self::Unavailable => "skia-unavailable",
        }
    }

    fn fallback_reason(&self) -> Option<&'static str> {
        match self {
            Self::Software {
                fallback_reason, ..
            } => *fallback_reason,
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(_) => None,
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(_) => None,
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(_) => None,
            Self::Unavailable => None,
        }
    }

    fn set_fallback_reason(&mut self, reason: &'static str) {
        if let Self::Software {
            fallback_reason, ..
        } = self
        {
            *fallback_reason = Some(reason);
        }
    }

    fn is_gpu(&self) -> bool {
        match self {
            Self::Software { .. } | Self::Unavailable => false,
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(_) => true,
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(_) => true,
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(_) => true,
        }
    }

    fn draw_and_present(
        &mut self,
        scene: &crate::core::Scene,
        frame: &FrameInfo<'_>,
        damage: &[PhysicalRect],
    ) -> Result<WinitFrameTimings, WinitRenderError> {
        match self {
            Self::Software {
                surface, renderer, ..
            } => {
                let draw_started = Instant::now();
                renderer.draw(scene, frame).map_err(|error| {
                    WinitRenderError::new(
                        crate::renderer::RenderErrorStage::Draw,
                        "skia_software_draw",
                        error,
                    )
                })?;
                let draw_ms = draw_started.elapsed().as_secs_f32() * 1_000.0;
                let acquire_started = Instant::now();
                let (width, height) = renderer.size();
                let width_nz = NonZeroU32::new(width.max(1) as u32).unwrap();
                let height_nz = NonZeroU32::new(height.max(1) as u32).unwrap();
                surface.resize(width_nz, height_nz).map_err(|error| {
                    WinitRenderError::new(
                        crate::renderer::RenderErrorStage::Prepare,
                        "softbuffer_resize",
                        error.to_string(),
                    )
                })?;
                let mut buffer = surface.buffer_mut().map_err(|error| {
                    WinitRenderError::new(
                        crate::renderer::RenderErrorStage::Prepare,
                        "softbuffer_buffer",
                        error.to_string(),
                    )
                })?;
                let acquire_ms = acquire_started.elapsed().as_secs_f32() * 1_000.0;
                for (destination, source) in
                    buffer.iter_mut().zip(renderer.pixels().chunks_exact(4))
                {
                    *destination =
                        ((source[2] as u32) << 16) | ((source[1] as u32) << 8) | source[0] as u32;
                }
                let damage = damage
                    .iter()
                    .filter_map(|rect| {
                        Some(softbuffer::Rect {
                            x: rect.left.max(0) as u32,
                            y: rect.top.max(0) as u32,
                            width: NonZeroU32::new(rect.width().max(0) as u32)?,
                            height: NonZeroU32::new(rect.height().max(0) as u32)?,
                        })
                    })
                    .collect::<Vec<_>>();
                let present_started = Instant::now();
                buffer.present_with_damage(&damage).map_err(|error| {
                    WinitRenderError::new(
                        crate::renderer::RenderErrorStage::Present,
                        "softbuffer_present",
                        error.to_string(),
                    )
                })?;
                Ok(WinitFrameTimings {
                    acquire_ms,
                    draw_ms,
                    flush_ms: 0.0,
                    present_ms: present_started.elapsed().as_secs_f32() * 1_000.0,
                })
            }
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(renderer) => renderer.draw_scene(scene, frame).map_err(|error| {
                WinitRenderError::new(
                    crate::renderer::RenderErrorStage::Present,
                    "skia_opengl_frame",
                    error,
                )
            }),
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(renderer) => renderer.draw_scene(scene, frame).map_err(|error| {
                WinitRenderError::new(
                    crate::renderer::RenderErrorStage::Present,
                    "skia_vulkan_frame",
                    error,
                )
            }),
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(renderer) => renderer.draw_scene(scene, frame).map_err(|error| {
                WinitRenderError::new(
                    crate::renderer::RenderErrorStage::Present,
                    "skia_metal_frame",
                    error,
                )
            }),
            Self::Unavailable => Err(WinitRenderError::new(
                crate::renderer::RenderErrorStage::Create,
                "renderer_unavailable",
                "renderer recreation has not completed",
            )),
        }
    }

    #[cfg(feature = "diagnostics")]
    fn device_info(&self) -> crate::diagnostics::RendererDeviceInfo {
        match self {
            Self::Software { .. } => crate::diagnostics::RendererDeviceInfo {
                api: "software".to_owned(),
                color_format: "BGRA8 premultiplied".to_owned(),
                present_mode: "softbuffer damage".to_owned(),
                ..crate::diagnostics::RendererDeviceInfo::default()
            },
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(renderer) => renderer.device_info(),
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(renderer) => renderer.device_info(),
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(renderer) => renderer.device_info(),
            Self::Unavailable => crate::diagnostics::RendererDeviceInfo::default(),
        }
    }

    fn trim(&mut self, pressure: MemoryPressure) {
        match self {
            Self::Software { renderer, .. } => renderer.trim(pressure),
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(renderer) => renderer.trim(pressure),
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(renderer) => renderer.trim(pressure),
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(renderer) => renderer.trim(pressure),
            Self::Unavailable => {}
        }
    }

    fn cache_stats(&self) -> super::skia::SkiaCacheStats {
        match self {
            Self::Software { renderer, .. } => renderer.cache_stats(),
            #[cfg(feature = "renderer-skia-gl")]
            Self::OpenGl(renderer) => renderer.cache_stats(),
            #[cfg(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))]
            Self::Vulkan(renderer) => renderer.cache_stats(),
            #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
            Self::Metal(renderer) => renderer.cache_stats(),
            Self::Unavailable => super::skia::SkiaCacheStats::default(),
        }
    }
}

fn renderer_recovery_state(renderer: &WinitSkiaRenderer) -> RendererRecoveryState {
    renderer
        .fallback_reason()
        .map_or(RendererRecoveryState::Healthy, |reason| {
            RendererRecoveryState::Fallback {
                reason,
                attempts: 0,
            }
        })
}

fn recovery_preference(
    configured: GraphicsPreference,
    was_fallback: bool,
    gpu_failure: bool,
    attempt: u8,
) -> (GraphicsPreference, bool) {
    let use_software =
        configured == GraphicsPreference::Auto && (was_fallback || (gpu_failure && attempt >= 2));
    if use_software {
        (GraphicsPreference::Software, true)
    } else {
        (configured, false)
    }
}

fn graphics_preference_supported(preference: GraphicsPreference) -> bool {
    match preference {
        GraphicsPreference::Auto | GraphicsPreference::Software => true,
        GraphicsPreference::OpenGl => cfg!(feature = "renderer-skia-gl"),
        GraphicsPreference::Vulkan => cfg!(all(
            feature = "renderer-skia-vulkan",
            any(target_os = "windows", target_os = "linux")
        )),
        GraphicsPreference::Metal => {
            cfg!(all(feature = "renderer-skia-metal", target_os = "macos"))
        }
    }
}

#[cfg(test)]
fn auto_driver_order() -> Vec<GraphicsPreference> {
    let mut drivers = Vec::with_capacity(3);
    #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
    drivers.push(GraphicsPreference::Metal);
    #[cfg(all(feature = "renderer-skia-vulkan", target_os = "linux"))]
    drivers.push(GraphicsPreference::Vulkan);
    #[cfg(feature = "renderer-skia-gl")]
    drivers.push(GraphicsPreference::OpenGl);
    drivers.push(GraphicsPreference::Software);
    drivers
}

fn create_renderer(
    preference: GraphicsPreference,
    soft_context: &SoftContext<OwnedDisplayHandle>,
    window: Arc<Window>,
    transparent: bool,
) -> Result<WinitSkiaRenderer, WinitApplicationError> {
    let mut fallback_reason = None;

    #[cfg(all(feature = "renderer-skia-metal", target_os = "macos"))]
    if matches!(
        preference,
        GraphicsPreference::Auto | GraphicsPreference::Metal
    ) {
        match super::winit_skia_metal::WinitMetalRenderer::new(
            Arc::clone(&window),
            DEFAULT_CACHE_BUDGET,
            transparent,
        ) {
            Ok(renderer) => return Ok(WinitSkiaRenderer::Metal(renderer)),
            Err(error) if preference == GraphicsPreference::Metal => {
                return Err(WinitApplicationError(error));
            }
            Err(error) => {
                eprintln!("lgui: Skia Metal initialization failed: {error}");
                fallback_reason = Some("metal-init-failed");
            }
        }
    }

    #[cfg(all(
        feature = "renderer-skia-vulkan",
        any(target_os = "windows", target_os = "linux")
    ))]
    if preference == GraphicsPreference::Vulkan
        || (preference == GraphicsPreference::Auto && cfg!(target_os = "linux"))
    {
        match super::winit_skia_vulkan::WinitVulkanRenderer::new(
            Arc::clone(&window),
            DEFAULT_CACHE_BUDGET,
            transparent,
        ) {
            Ok(renderer) => return Ok(WinitSkiaRenderer::Vulkan(renderer)),
            Err(error) if preference == GraphicsPreference::Vulkan => {
                return Err(WinitApplicationError(error));
            }
            Err(error) => {
                eprintln!("lgui: Skia Vulkan initialization failed: {error}");
                fallback_reason = Some("vulkan-init-failed");
            }
        }
    }

    #[cfg(feature = "renderer-skia-gl")]
    if matches!(
        preference,
        GraphicsPreference::Auto | GraphicsPreference::OpenGl
    ) {
        match super::winit_skia_gl::WinitOpenGlRenderer::new(
            Arc::clone(&window),
            DEFAULT_CACHE_BUDGET,
            transparent,
        ) {
            Ok(renderer) => return Ok(WinitSkiaRenderer::OpenGl(renderer)),
            Err(error) if preference == GraphicsPreference::OpenGl => {
                return Err(WinitApplicationError(error));
            }
            Err(error) => {
                eprintln!("lgui: Skia OpenGL initialization failed: {error}");
                fallback_reason = Some("opengl-init-failed");
            }
        }
    }

    if !matches!(
        preference,
        GraphicsPreference::Auto | GraphicsPreference::Software
    ) {
        return Err(WinitApplicationError(format!(
            "the {} Skia driver is not available on this target",
            preference.as_str()
        )));
    }
    let surface = SoftSurface::new(soft_context, window)
        .map_err(|error| WinitApplicationError(format!("create software surface: {error}")))?;
    Ok(WinitSkiaRenderer::Software {
        surface,
        renderer: SkiaSoftwareSurface::new(DEFAULT_CACHE_BUDGET),
        fallback_reason: fallback_reason.or_else(|| {
            (preference == GraphicsPreference::Auto).then_some("gpu-driver-unavailable")
        }),
    })
}

fn initial_scale(event_loop: &ActiveEventLoop, options: &WindowOptions) -> UiScale {
    scale_for_monitor(event_loop.primary_monitor().as_ref(), options)
}

fn scale_for_monitor(
    monitor: Option<&winit::monitor::MonitorHandle>,
    options: &WindowOptions,
) -> UiScale {
    let Some(monitor) = monitor else {
        return UiScale::ONE;
    };
    let position = monitor.position();
    let size = monitor.size();
    let work = WorkArea {
        rect: PhysicalRect::new(
            position.x,
            position.y,
            position.x + size.width as i32,
            position.y + size.height as i32,
        ),
    };
    ScaleContext::resolve(
        (monitor.scale_factor() * BASE_DPI as f64).round() as u32,
        work,
        options.scale_reference_size.unwrap_or(options.size),
        options.scale_preference,
    )
    .scale
}

fn position_window(
    window: &Window,
    options: &WindowOptions,
    owner: Option<&Window>,
    cursor: Option<PhysicalPosition<i32>>,
) {
    if let WindowPosition::AdjacentToOwner { gap } = options.position {
        if let Some(owner) = owner {
            if let (Ok(origin), size) = (owner.outer_position(), owner.outer_size()) {
                let target = window.outer_size();
                let mut x = origin.x + size.width as i32 + gap;
                let mut y = origin.y;
                if let Some(monitor) = owner.current_monitor() {
                    let monitor_origin = monitor.position();
                    let monitor_size = monitor.size();
                    let right = monitor_origin.x + monitor_size.width as i32;
                    let bottom = monitor_origin.y + monitor_size.height as i32;
                    if x + target.width as i32 > right {
                        x = origin.x - target.width as i32 - gap;
                    }
                    x = x.clamp(
                        monitor_origin.x,
                        (right - target.width as i32).max(monitor_origin.x),
                    );
                    y = y.clamp(
                        monitor_origin.y,
                        (bottom - target.height as i32).max(monitor_origin.y),
                    );
                }
                window.set_outer_position(PhysicalPosition::new(x, y));
                return;
            }
        }
    }
    if let WindowPosition::NearCursor { gap } = options.position {
        if let Some(cursor) = cursor {
            let target = window.outer_size();
            let mut x = cursor.x + gap;
            let mut y = cursor.y + gap;
            if let Some(monitor) = window.current_monitor() {
                let origin = monitor.position();
                let size = monitor.size();
                let right = origin.x + size.width as i32;
                let bottom = origin.y + size.height as i32;
                if x + target.width as i32 > right {
                    x = cursor.x - target.width as i32 - gap;
                }
                if y + target.height as i32 > bottom {
                    y = cursor.y - target.height as i32 - gap;
                }
                x = x.clamp(origin.x, (right - target.width as i32).max(origin.x));
                y = y.clamp(origin.y, (bottom - target.height as i32).max(origin.y));
            }
            window.set_outer_position(PhysicalPosition::new(x, y));
            return;
        }
    }
    if !matches!(options.position, WindowPosition::Centered) {
        return;
    }
    let Some(monitor) = window.current_monitor() else {
        return;
    };
    let origin = monitor.position();
    let area = monitor.size();
    let size = window.outer_size();
    window.set_outer_position(PhysicalPosition::new(
        origin.x + (area.width as i32 - size.width as i32) / 2,
        origin.y + (area.height as i32 - size.height as i32) / 2,
    ));
}

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        MouseButton::Other(value) => PointerButton::Other(value),
    }
}

fn resize_direction(
    point: PhysicalPoint,
    size: WinitPhysicalSize<u32>,
    border: i32,
) -> Option<ResizeDirection> {
    let left = point.x >= 0 && point.x < border;
    let right = point.x < size.width as i32 && point.x >= size.width as i32 - border;
    let top = point.y >= 0 && point.y < border;
    let bottom = point.y < size.height as i32 && point.y >= size.height as i32 - border;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (_, true, true, _) => Some(ResizeDirection::NorthEast),
        (true, _, _, true) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, _, _) => Some(ResizeDirection::West),
        (_, true, _, _) => Some(ResizeDirection::East),
        (_, _, true, _) => Some(ResizeDirection::North),
        (_, _, _, true) => Some(ResizeDirection::South),
        _ => None,
    }
}

fn touch_phase(phase: WinitTouchPhase) -> TouchPhase {
    match phase {
        WinitTouchPhase::Started => TouchPhase::Started,
        WinitTouchPhase::Moved => TouchPhase::Moved,
        WinitTouchPhase::Ended => TouchPhase::Ended,
        WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
    }
}

fn keyboard_event(event: winit::event::KeyEvent, modifiers: ModifiersState) -> KeyboardEvent {
    let key = match event.logical_key {
        WinitKey::Character(value) => LogicalKey::Character(value.to_string()),
        WinitKey::Named(value) => format!("{value:?}")
            .parse()
            .map(LogicalKey::Named)
            .unwrap_or_else(|_| LogicalKey::Character(format!("{value:?}"))),
        WinitKey::Unidentified(_) => LogicalKey::Character(String::new()),
        WinitKey::Dead(value) => {
            LogicalKey::Character(value.map_or_else(String::new, |value| value.to_string()))
        }
    };
    let code = match event.physical_key {
        WinitPhysicalKey::Code(value) => format!("{value:?}").parse().unwrap_or_default(),
        WinitPhysicalKey::Unidentified(_) => PhysicalKey::default(),
    };
    let location = match event.location {
        WinitKeyLocation::Standard => KeyLocation::Standard,
        WinitKeyLocation::Left => KeyLocation::Left,
        WinitKeyLocation::Right => KeyLocation::Right,
        WinitKeyLocation::Numpad => KeyLocation::Numpad,
    };
    let mut translated = KeyModifiers::empty();
    if modifiers.alt_key() {
        translated |= KeyModifiers::ALT;
    }
    if modifiers.control_key() {
        translated |= KeyModifiers::CONTROL;
    }
    if modifiers.shift_key() {
        translated |= KeyModifiers::SHIFT;
    }
    if modifiers.super_key() {
        translated |= KeyModifiers::META;
    }
    KeyboardEvent {
        state: match event.state {
            ElementState::Pressed => KeyState::Down,
            ElementState::Released => KeyState::Up,
        },
        key,
        code,
        location,
        modifiers: translated,
        repeat: event.repeat,
        is_composing: false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::*;

    #[test]
    fn software_backend_rejects_an_explicit_unavailable_driver() {
        let backend = WinitApplication::new(GraphicsPreference::Vulkan);
        assert_eq!(backend.preference, GraphicsPreference::Vulkan);
    }

    #[test]
    fn mouse_buttons_keep_extended_button_identity() {
        assert_eq!(pointer_button(MouseButton::Back), PointerButton::Back);
        assert_eq!(
            pointer_button(MouseButton::Other(9)),
            PointerButton::Other(9)
        );
    }

    #[test]
    fn auto_recovery_is_bounded_before_software_fallback() {
        assert_eq!(
            recovery_preference(GraphicsPreference::Auto, false, true, 1),
            (GraphicsPreference::Auto, false)
        );
        assert_eq!(
            recovery_preference(GraphicsPreference::Auto, false, true, 2),
            (GraphicsPreference::Software, true)
        );
    }

    #[test]
    fn explicit_gpu_recovery_never_silently_falls_back() {
        assert_eq!(
            recovery_preference(GraphicsPreference::OpenGl, false, true, 3),
            (GraphicsPreference::OpenGl, false)
        );
    }

    #[test]
    fn driver_support_matches_compiled_winit_backends() {
        assert!(graphics_preference_supported(GraphicsPreference::Auto));
        assert!(graphics_preference_supported(GraphicsPreference::Software));
        assert_eq!(
            graphics_preference_supported(GraphicsPreference::OpenGl),
            cfg!(feature = "renderer-skia-gl")
        );
        assert_eq!(
            graphics_preference_supported(GraphicsPreference::Vulkan),
            cfg!(all(
                feature = "renderer-skia-vulkan",
                any(target_os = "windows", target_os = "linux")
            ))
        );
        assert_eq!(
            graphics_preference_supported(GraphicsPreference::Metal),
            cfg!(all(feature = "renderer-skia-metal", target_os = "macos"))
        );
    }

    #[test]
    fn auto_driver_order_matches_each_platform_policy() {
        let order = auto_driver_order();
        assert_eq!(order.last(), Some(&GraphicsPreference::Software));
        #[cfg(target_os = "windows")]
        assert_eq!(
            order.first(),
            if cfg!(feature = "renderer-skia-gl") {
                Some(&GraphicsPreference::OpenGl)
            } else {
                Some(&GraphicsPreference::Software)
            }
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            order.first(),
            if cfg!(feature = "renderer-skia-vulkan") {
                Some(&GraphicsPreference::Vulkan)
            } else if cfg!(feature = "renderer-skia-gl") {
                Some(&GraphicsPreference::OpenGl)
            } else {
                Some(&GraphicsPreference::Software)
            }
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            order.first(),
            if cfg!(feature = "renderer-skia-metal") {
                Some(&GraphicsPreference::Metal)
            } else if cfg!(feature = "renderer-skia-gl") {
                Some(&GraphicsPreference::OpenGl)
            } else {
                Some(&GraphicsPreference::Software)
            }
        );
    }

    #[test]
    fn auxiliary_windows_inherit_the_main_owner_and_application_context() {
        let context = ApplicationContext::empty();
        let expected = context.clone();
        let rendered = Arc::new(AtomicBool::new(false));
        let rendered_view = Arc::clone(&rendered);
        let view: AppView = Arc::new(move |cx| {
            assert!(cx.application() == expected);
            rendered_view.store(true, Ordering::Relaxed);
            crate::core::content_text("auxiliary")
        });
        let (options, view) = prepare_auxiliary_window(
            &context,
            &WindowId::new("main"),
            WindowOptions::new("chat"),
            view,
        );

        assert_eq!(options.owner, Some(WindowId::new("main")));
        let mut session = UiSession::new();
        let _ = session.render_view(&view, UiRect::new(0.0, 0.0, 320.0, 240.0), UiScale::ONE);
        assert!(rendered.load(Ordering::Relaxed));
    }

    #[test]
    fn auxiliary_windows_preserve_an_explicit_owner() {
        let context = ApplicationContext::empty();
        let view: AppView = Arc::new(|_| crate::core::content_text("auxiliary"));
        let (options, _) = prepare_auxiliary_window(
            &context,
            &WindowId::new("main"),
            WindowOptions::new("chat").owner("workspace"),
            view,
        );

        assert_eq!(options.owner, Some(WindowId::new("workspace")));
    }
}
