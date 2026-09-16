use super::*;

pub(crate) enum WinitUserEvent {
    Task(ApplicationTask),
    Window(WindowCommand),
    Wake(WindowId),
    RequestAllFrames,
    MemoryTrim(
        WindowId,
        lgui_core::memory::CacheDomain,
        lgui_core::memory::TrimRequest,
    ),
    MemoryBudget(WindowId, usize),
    #[cfg(all(feature = "tray-win32", target_os = "windows"))]
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

pub(super) struct WinitHost {
    pub(super) initial: Option<(WindowOptions, AppView)>,
    pub(super) main_id: WindowId,
    pub(super) context: ApplicationContext,
    pub(super) proxy: EventLoopProxy<WinitUserEvent>,
    pub(super) soft_context: Arc<SoftContext<OwnedDisplayHandle>>,
    pub(super) windows: HashMap<WinitWindowId, WinitWindow>,
    pub(super) ids: HashMap<WindowId, WinitWindowId>,
    pub(super) scale_preference: Option<lgui_core::ScalePreference>,
    pub(super) preference: GraphicsPreference,
    #[cfg(all(feature = "tray-win32", target_os = "windows"))]
    pub(super) tray_host: Option<lgui_platform_win32::Win32TrayHost>,
    pub(super) exit_requested: bool,
    pub(super) backgrounded: bool,
}

impl ApplicationHandler<WinitUserEvent> for WinitHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some((options, view)) = self.initial.take() {
            #[cfg(all(feature = "tray-win32", target_os = "windows"))]
            let main_id = options.id.clone();
            if let Err(error) = self.create_window(event_loop, options, view) {
                lgui_core::backend::report_render_error(
                    &self.context,
                    lgui_core::backend::render_error(
                        WindowId::new("main"),
                        "skia-software",
                        lgui_render_api::RenderErrorStage::Create,
                        "create_winit_window",
                        -1,
                        error.to_string(),
                    ),
                );
                self.exit_requested = true;
                event_loop.exit();
                return;
            }
            #[cfg(all(feature = "tray-win32", target_os = "windows"))]
            if let Some(registration) = self.context.try_resource::<TrayRegistration>() {
                let proxy = self.proxy.clone();
                let visible_id = main_id.clone();
                let action_context = self.context.clone();
                let action_registration = Arc::clone(&registration);
                match lgui_platform_win32::Win32TrayHost::spawn(
                    registration.options.clone(),
                    move |action| {
                        dispatch_tray_action(
                            &action_registration,
                            &action_context,
                            action,
                            |visible| {
                                let _ = proxy.send_event(WinitUserEvent::SetVisible(
                                    visible_id.clone(),
                                    visible,
                                ));
                            },
                        );
                    },
                ) {
                    Ok(host) => self.tray_host = Some(host),
                    Err(error) => lgui_core::backend::report_render_error(
                        &self.context,
                        lgui_core::backend::render_error(
                            main_id,
                            "platform-winit",
                            lgui_render_api::RenderErrorStage::Create,
                            "create_tray_host",
                            -1,
                            error.to_string(),
                        ),
                    ),
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
            WinitUserEvent::MemoryTrim(id, domain, request) => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.apply_memory_trim(domain, request);
                }
            }
            WinitUserEvent::MemoryBudget(id, budget) => {
                if let Some(window) = self.window_by_id_mut(&id) {
                    window.set_memory_budget(budget);
                }
            }
            #[cfg(all(feature = "tray-win32", target_os = "windows"))]
            WinitUserEvent::SetVisible(id, visible) => {
                self.set_window_visibility(&id, visible);
                if visible {
                    if let Some(window) = self.window_by_id_mut(&id) {
                        window.window.focus_window();
                    }
                }
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
        self.refresh_background_state();
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

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.context
            .memory()
            .notify(lgui_core::memory::MemoryEvent::ApplicationShutdown);
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
        let renderer_budget = self
            .context
            .memory()
            .options()
            .domain_budget(lgui_core::memory::CacheDomain::Skia)
            .checked_div(self.windows.len().saturating_add(1))
            .unwrap_or(0);
        let renderer = create_renderer(
            self.preference,
            &self.soft_context,
            Arc::clone(&window),
            options.transparent,
            renderer_budget,
        )?;
        let mut session = UiSession::new();
        if let Some(executor) = lgui_core::backend::task_spawner(&self.context) {
            session.set_task_spawner(executor);
        }
        let id = options.id.clone();
        let memory_instance = self.context.memory().next_instance_id();
        let memory_usage = Arc::new(Mutex::new(lgui_core::memory::CacheUsage::default()));
        let stats_usage = Arc::clone(&memory_usage);
        let trim_usage = Arc::clone(&memory_usage);
        let trim_proxy = self.proxy.clone();
        let trim_id = id.clone();
        let memory_registration =
            self.context
                .memory()
                .register(lgui_core::memory::DomainRegistration::new(
                    lgui_core::memory::CacheDomain::Skia,
                    memory_instance,
                    format!("window:{}", id.as_str()),
                    lgui_core::memory::CacheAdapter::managed(
                        move || *stats_usage.lock().expect("Skia memory usage poisoned"),
                        move |request| {
                            let before = trim_usage
                                .lock()
                                .expect("Skia memory usage poisoned")
                                .resident_bytes();
                            let _ = trim_proxy.send_event(WinitUserEvent::MemoryTrim(
                                trim_id.clone(),
                                lgui_core::memory::CacheDomain::Skia,
                                request,
                            ));
                            lgui_core::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        {
                            let budget_proxy = self.proxy.clone();
                            let budget_id = id.clone();
                            move |budget| {
                                let _ = budget_proxy.send_event(WinitUserEvent::MemoryBudget(
                                    budget_id.clone(),
                                    budget,
                                ));
                            }
                        },
                    ),
                ));
        let component_memory = Arc::new(Mutex::new(lgui_core::memory::CacheUsage::default()));
        let host_scene_memory = Arc::new(Mutex::new(lgui_core::memory::CacheUsage::default()));
        let component_registration = {
            let usage = Arc::clone(&component_memory);
            let trim_usage = Arc::clone(&component_memory);
            let proxy = self.proxy.clone();
            let id = id.clone();
            self.context
                .memory()
                .register(lgui_core::memory::DomainRegistration::new(
                    lgui_core::memory::CacheDomain::ComponentOutput,
                    memory_instance,
                    format!("window:{}", id.as_str()),
                    lgui_core::memory::CacheAdapter::new(
                        move || *usage.lock().expect("component memory usage poisoned"),
                        move |request| {
                            let before = trim_usage
                                .lock()
                                .expect("component memory usage poisoned")
                                .resident_bytes();
                            let _ = proxy.send_event(WinitUserEvent::MemoryTrim(
                                id.clone(),
                                lgui_core::memory::CacheDomain::ComponentOutput,
                                request,
                            ));
                            lgui_core::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                    ),
                ))
        };
        let host_scene_registration = {
            let usage = Arc::clone(&host_scene_memory);
            let trim_usage = Arc::clone(&host_scene_memory);
            let proxy = self.proxy.clone();
            let id = id.clone();
            self.context
                .memory()
                .register(lgui_core::memory::DomainRegistration::new(
                    lgui_core::memory::CacheDomain::HostScene,
                    memory_instance,
                    format!("window:{}", id.as_str()),
                    lgui_core::memory::CacheAdapter::new(
                        move || *usage.lock().expect("host scene memory usage poisoned"),
                        move |request| {
                            let before = trim_usage
                                .lock()
                                .expect("host scene memory usage poisoned")
                                .resident_bytes();
                            let _ = proxy.send_event(WinitUserEvent::MemoryTrim(
                                id.clone(),
                                lgui_core::memory::CacheDomain::HostScene,
                                request,
                            ));
                            lgui_core::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                    ),
                ))
        };
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
                memory_instance,
                memory_budget: renderer_budget,
                memory_usage,
                _memory_registration: memory_registration,
                component_memory,
                host_scene_memory,
                _component_registration: component_registration,
                _host_scene_registration: host_scene_registration,
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
                    window.set_desired_visibility(false);
                }
                self.suppress_owned_windows(&id, true);
                self.refresh_background_state();
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
                self.context
                    .memory()
                    .notify(lgui_core::memory::MemoryEvent::ThemeOrScaleChanged);
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
            lgui_core::backend::report_render_error(
                &self.context,
                lgui_core::backend::render_error(
                    id,
                    "platform-winit",
                    lgui_render_api::RenderErrorStage::Create,
                    "create_auxiliary_window",
                    -1,
                    error.to_string(),
                ),
            );
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
                    window.set_desired_visibility(false);
                    window.id.clone()
                };
                self.suppress_owned_windows(&id, true);
                self.refresh_background_state();
            }
            ClosePolicy::Notify => {
                let window = self.windows.get_mut(&native).expect("window exists");
                if let Some(handler) = window.options.close_handler {
                    let mut event = lgui_core::core::UiEventContext::new(
                        window.context.clone(),
                        window.id.clone(),
                    );
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
            window.set_desired_visibility(visible);
        }
        self.suppress_owned_windows(id, !visible);
        self.refresh_background_state();
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
                window.set_owner_suppressed(suppressed);
            }
            self.suppress_owned_windows(&child, suppressed);
        }
    }

    fn refresh_background_state(&mut self) {
        let roots = self
            .windows
            .values()
            .filter(|window| window.options.owner.is_none())
            .collect::<Vec<_>>();
        let backgrounded = !roots.is_empty()
            && roots.iter().all(|window| !window.visible)
            && roots
                .iter()
                .any(|window| window.options.background_memory_optimization);
        if backgrounded {
            for window in self.windows.values_mut() {
                window.suspend_rendering(true);
            }
        }
        if backgrounded != self.backgrounded {
            self.context.memory().notify(if backgrounded {
                lgui_core::memory::MemoryEvent::AllWindowsHidden
            } else {
                lgui_core::memory::MemoryEvent::WindowShown
            });
            self.backgrounded = backgrounded;
        }
    }
}
