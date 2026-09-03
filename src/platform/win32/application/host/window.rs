use super::*;

pub(super) fn create_window(
    instance: HINSTANCE,
    class_name: &[u16],
    options: WindowOptions,
    view: AppView,
    context: ApplicationContext,
    renderer_factory: Arc<dyn Win32RendererFactory>,
    dispatcher: Win32Dispatcher,
) -> Result<HWND> {
    let initial_mode = options.mode;
    let initially_visible = options.visible;
    let title = wide(&options.title);
    let style = window_style(&options);
    let owner = if let Some(owner_id) = options.owner.as_ref() {
        Some(
            hwnd_for_id(owner_id)
                .ok_or_else(|| Error::from_hresult(HRESULT(0x80070057_u32 as i32)))?,
        )
    } else {
        STATE.with(|state| {
            state
                .borrow()
                .iter()
                .find_map(|(raw, state)| state.owner.is_none().then_some(HWND(*raw as _)))
        })
    };
    let mut ex_style = WINDOW_EX_STYLE(0);
    if owner.is_some() {
        ex_style |= WS_EX_TOOLWINDOW;
    }
    if options.transparent {
        ex_style |= WS_EX_LAYERED;
    }
    if options.topmost {
        ex_style |= WS_EX_TOPMOST;
    }
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            options.size.width.ceil() as i32,
            options.size.height.ceil() as i32,
            owner,
            None,
            Some(instance),
            None,
        )
    }?;
    install_window_icons(hwnd);
    if !options.native_titlebar {
        set_runtime_window_style(hwnd, style);
    }
    set_window_corner_preference(hwnd, options.corner_radius, initial_mode);
    let renderer_name = renderer_factory.name();
    let mut renderer = renderer_factory.create(hwnd).map_err(|source| {
        context.report_render_error(RenderError::new(
            options.id.clone(),
            renderer_name,
            RenderErrorStage::Create,
            "create_renderer",
            source.code().0,
            source.to_string(),
        ));
        source
    })?;
    let memory_domain = if renderer_name == "d2d" {
        crate::memory::CacheDomain::D2d
    } else {
        crate::memory::CacheDomain::Gdi
    };
    let renderer_budget = Arc::new(AtomicUsize::new(
        context.memory().options().domain_budget(memory_domain),
    ));
    renderer.set_memory_budget(renderer_budget.load(Ordering::Acquire));
    let renderer_memory = Arc::new(Mutex::new(renderer.memory_usage()));
    let stats_memory = Arc::clone(&renderer_memory);
    let trim_memory = Arc::clone(&renderer_memory);
    let trim_budget = Arc::clone(&renderer_budget);
    let set_budget = Arc::clone(&renderer_budget);
    let budget_dispatcher = dispatcher.clone();
    let raw_hwnd = hwnd.0 as isize;
    let trim = CoalescedTrim::new(dispatcher.clone(), move |target_bytes| {
        STATE.with(|state| {
            let mut windows = state.borrow_mut();
            let Some(window) = windows.get_mut(&raw_hwnd) else {
                return 0;
            };
            if let Some(renderer) = window.renderer.as_mut() {
                let before = renderer.memory_usage().resident_bytes();
                renderer.trim_to(target_bytes);
                let usage = renderer.memory_usage();
                *window
                    .renderer_memory
                    .lock()
                    .expect("renderer memory usage poisoned") = usage;
                before.saturating_sub(usage.resident_bytes())
            } else {
                0
            }
        })
    });
    let memory_instance = context.memory().next_instance_id();
    let renderer_memory_registration =
        context
            .memory()
            .register(crate::memory::DomainRegistration::new(
                memory_domain,
                memory_instance,
                format!("window:{}", options.id.as_str()),
                crate::memory::CacheAdapter::managed(
                    move || *stats_memory.lock().expect("renderer memory usage poisoned"),
                    move |request| {
                        let before = trim_memory
                            .lock()
                            .expect("renderer memory usage poisoned")
                            .resident_bytes();
                        let released = trim.run_or_request(request.target_bytes).unwrap_or(0);
                        crate::memory::TrimResult {
                            before_bytes: before,
                            after_bytes: before.saturating_sub(released),
                        }
                    },
                    move |budget| {
                        set_budget.store(budget, Ordering::Release);
                        budget_dispatcher.post(move || {
                            STATE.with(|state| {
                                let mut windows = state.borrow_mut();
                                let Some(window) = windows.get_mut(&raw_hwnd) else {
                                    return;
                                };
                                if let Some(renderer) = window.renderer.as_mut() {
                                    renderer.set_memory_budget(budget);
                                    *window
                                        .renderer_memory
                                        .lock()
                                        .expect("renderer memory usage poisoned") =
                                        renderer.memory_usage();
                                }
                            });
                        });
                    },
                ),
            ));
    let mut session = UiSession::new();
    if let Some(executor) = context.task_spawner() {
        session.set_task_spawner(executor);
    }
    let (component_usage, host_scene_usage) = session.memory_usage();
    let component_memory = Arc::new(Mutex::new(component_usage));
    let host_scene_memory = Arc::new(Mutex::new(host_scene_usage));
    let component_memory_registration = {
        let usage = Arc::clone(&component_memory);
        let trim_usage = Arc::clone(&component_memory);
        let trim = CoalescedTrim::new(dispatcher.clone(), move |target_bytes| {
            STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let Some(window) = windows.get_mut(&raw_hwnd) else {
                    return 0;
                };
                let released = window.session.trim_component_outputs(target_bytes);
                update_session_memory_usage(window);
                released
            })
        });
        context
            .memory()
            .register(crate::memory::DomainRegistration::new(
                crate::memory::CacheDomain::ComponentOutput,
                memory_instance,
                format!("window:{}", options.id.as_str()),
                crate::memory::CacheAdapter::new(
                    move || *usage.lock().expect("component memory usage poisoned"),
                    move |request| {
                        let before = trim_usage
                            .lock()
                            .expect("component memory usage poisoned")
                            .resident_bytes();
                        let released = trim.run_or_request(request.target_bytes).unwrap_or(0);
                        crate::memory::TrimResult {
                            before_bytes: before,
                            after_bytes: before.saturating_sub(released),
                        }
                    },
                ),
            ))
    };
    let host_scene_memory_registration = {
        let usage = Arc::clone(&host_scene_memory);
        let trim_usage = Arc::clone(&host_scene_memory);
        let trim = CoalescedTrim::new(dispatcher.clone(), move |_target_bytes| {
            STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let Some(window) = windows.get_mut(&raw_hwnd) else {
                    return 0;
                };
                let released = window.session.trim_host_scene();
                update_session_memory_usage(window);
                released
            })
        });
        context
            .memory()
            .register(crate::memory::DomainRegistration::new(
                crate::memory::CacheDomain::HostScene,
                memory_instance,
                format!("window:{}", options.id.as_str()),
                crate::memory::CacheAdapter::new(
                    move || *usage.lock().expect("host scene memory usage poisoned"),
                    move |request| {
                        let before = trim_usage
                            .lock()
                            .expect("host scene memory usage poisoned")
                            .resident_bytes();
                        let released = should_trim_host_scene(request)
                            .then(|| trim.run_or_request(request.target_bytes).unwrap_or(0))
                            .unwrap_or(0);
                        crate::memory::TrimResult {
                            before_bytes: before,
                            after_bytes: before.saturating_sub(released),
                        }
                    },
                ),
            ))
    };
    #[cfg(feature = "diagnostics")]
    let diagnostics = context.try_resource::<DiagnosticsRegistration>();
    STATE.with(|state| {
        state.borrow_mut().insert(
            hwnd.0 as isize,
            WindowState {
                id: options.id,
                view,
                context,
                session,
                renderer: Some(renderer),
                renderer_memory,
                renderer_budget: trim_budget,
                #[cfg(feature = "images-win32")]
                memory_instance,
                #[cfg(feature = "images-win32")]
                image_reachability_scene: None,
                _renderer_memory_registration: renderer_memory_registration,
                component_memory,
                host_scene_memory,
                _component_memory_registration: component_memory_registration,
                _host_scene_memory_registration: host_scene_memory_registration,
                renderer_factory,
                logical_size: options.size,
                minimum_size: options.minimum_size,
                maximum_size: options.maximum_size,
                resizable: options.resizable,
                native_titlebar: options.native_titlebar,
                corner_radius: options.corner_radius,
                titlebar_drag_height: options.titlebar_drag_height,
                drag_exclusion: options.drag_exclusion,
                windowed_style: style,
                windowed_placement: None,
                mode: WindowMode::Windowed,
                owner,
                position: options.position,
                hide_on_deactivate: options.hide_on_deactivate,
                background_memory_optimization: options.background_memory_optimization,
                rendering_suspended: false,
                interaction_mode: WindowInteractionMode::Idle,
                resize_frame_throttle: ResizeFrameThrottle::default(),
                visibility: OwnerVisibility {
                    desired_visible: initially_visible,
                    hidden_for_owner: false,
                },
                close_policy: options.close_policy,
                close_handler: options.close_handler,
                dispatcher,
                #[cfg(feature = "diagnostics")]
                diagnostics,
                render_retry_used: false,
                #[cfg(feature = "diagnostics")]
                frame_index: 0,
                suppressed_ime_char_units: VecDeque::new(),
                pending_high_surrogate: None,
                pointer_inside: false,
            },
        );
    });
    install_wake(hwnd);
    if options.transparent {
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        }
    }
    position_window(hwnd);
    if initial_mode == WindowMode::Fullscreen {
        set_window_mode(hwnd, WindowMode::Fullscreen);
    }
    if initially_visible {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
        }
    } else {
        render_hidden_window_once(hwnd);
    }
    Ok(hwnd)
}

pub(super) fn install_window_icons(hwnd: HWND) {
    for (kind, class_index) in [(ICON_BIG, GCLP_HICON), (ICON_SMALL, GCLP_HICONSM)] {
        let icon = unsafe { GetClassLongPtrW(hwnd, class_index) };
        if icon != 0 {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(icon as isize)),
                );
            }
        }
    }
}

pub(super) fn render_hidden_window_once(hwnd: HWND) {
    let target = unsafe { GetDC(Some(hwnd)) };
    if target.is_invalid() {
        return;
    }
    render_window(hwnd, target);
    unsafe {
        let _ = ReleaseDC(Some(hwnd), target);
    }
}

pub(super) fn window_style(options: &WindowOptions) -> WINDOW_STYLE {
    // A custom-framed window must be born as a popup. Creating an overlapped window and
    // removing WS_CAPTION afterwards lets Windows paint the native frame for one frame.
    let mut style = if options.native_titlebar {
        WS_OVERLAPPED | WS_CAPTION
    } else {
        WS_POPUP
    } | WS_SYSMENU
        | WS_MINIMIZEBOX;
    if options.resizable {
        style |= WS_THICKFRAME | WS_MAXIMIZEBOX;
    }
    style
}

pub(super) fn set_runtime_window_style(hwnd: HWND, style: WINDOW_STYLE) {
    let current = WINDOW_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32);
    let visibility = WINDOW_STYLE(current.0 & WS_VISIBLE.0);
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style | visibility).0 as isize);
    }
}

pub(super) fn set_window_corner_preference(hwnd: HWND, radius: i32, mode: WindowMode) {
    let preference = window_corner_preference(radius, mode);
    unsafe {
        // Windows versions before Windows 11 do not expose this attribute. The request is a
        // progressive enhancement, so an unsupported DWM attribute must not block creation.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

pub(super) fn window_corner_preference(
    radius: i32,
    mode: WindowMode,
) -> DWM_WINDOW_CORNER_PREFERENCE {
    if radius <= 0 || mode != WindowMode::Windowed {
        DWMWCP_DONOTROUND
    } else if radius <= 4 {
        windows::Win32::Graphics::Dwm::DWMWCP_ROUNDSMALL
    } else {
        DWMWCP_ROUND
    }
}

pub(super) enum WindowModeTransition {
    Fullscreen,
    Windowed {
        corner_radius: i32,
        style: WINDOW_STYLE,
        placement: Option<WINDOWPLACEMENT>,
    },
}

pub(super) fn set_window_mode(hwnd: HWND, mode: WindowMode) {
    let current_mode = STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|window| window.mode)
    });
    if current_mode.is_none() || current_mode == Some(mode) {
        return;
    }

    // Query native state before borrowing STATE because Win32 calls may synchronously re-enter
    // window_proc on this thread.
    let placement = if mode == WindowMode::Fullscreen {
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowPlacement(hwnd, &mut placement) }
            .is_ok()
            .then_some(placement)
    } else {
        None
    };

    // Commit the Rust-side transition first. Native calls stay below this closure so any window
    // messages they dispatch can borrow STATE normally.
    let transition = STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let window = windows.get_mut(&(hwnd.0 as isize))?;
        if window.mode == mode {
            return None;
        }
        let transition = match mode {
            WindowMode::Fullscreen => {
                window.windowed_placement = placement;
                WindowModeTransition::Fullscreen
            }
            WindowMode::Windowed => WindowModeTransition::Windowed {
                corner_radius: window.corner_radius,
                style: window.windowed_style,
                placement: window.windowed_placement.take(),
            },
        };
        window.mode = mode;
        Some(transition)
    });
    let Some(transition) = transition else {
        return;
    };

    match transition {
        WindowModeTransition::Fullscreen => {
            set_window_corner_preference(hwnd, 0, WindowMode::Fullscreen);
            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut monitor_info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool() {
                unsafe {
                    set_runtime_window_style(hwnd, WS_POPUP);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        monitor_info.rcMonitor.left,
                        monitor_info.rcMonitor.top,
                        monitor_info.rcMonitor.right - monitor_info.rcMonitor.left,
                        monitor_info.rcMonitor.bottom - monitor_info.rcMonitor.top,
                        SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER,
                    );
                }
            }
        }
        WindowModeTransition::Windowed {
            corner_radius,
            style,
            placement,
        } => {
            set_window_corner_preference(hwnd, corner_radius, WindowMode::Windowed);
            unsafe {
                set_runtime_window_style(hwnd, style);
                if let Some(placement) = placement {
                    let _ = SetWindowPlacement(hwnd, &placement);
                }
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED
                        | SWP_NOACTIVATE
                        | SWP_NOOWNERZORDER
                        | SWP_NOZORDER
                        | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                        | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
                );
            }
        }
    }
}

pub(super) fn hwnd_for_id(id: &WindowId) -> Option<HWND> {
    STATE.with(|state| {
        state
            .borrow()
            .iter()
            .find_map(|(raw, state)| (state.id == *id).then_some(HWND(*raw as _)))
    })
}

pub(super) fn set_desired_visibility(hwnd: HWND, visible: bool) {
    STATE.with(|state| {
        if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            window.visibility.set_desired(visible);
        }
    });
}

pub(super) fn hide_window(hwnd: HWND) {
    set_desired_visibility(hwnd, false);
    hide_owned_windows(hwnd);
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    suspend_window_rendering(hwnd, false);
    suspend_application_if_backgrounded();
}

pub(super) fn show_window(hwnd: HWND) {
    set_desired_visibility(hwnd, true);
    if !application_is_backgrounded() {
        BACKGROUND_TRIM_GENERATION.fetch_add(1, Ordering::AcqRel);
    }
    resume_window_rendering(hwnd);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    restore_owned_windows(hwnd);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = UpdateWindow(hwnd);
    }
}

pub(super) fn suspend_window_rendering(hwnd: HWND, force: bool) -> bool {
    let memory = STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return None;
        };
        if window.rendering_suspended || (!force && !window.background_memory_optimization) {
            return None;
        }
        window.renderer.take();
        *window
            .renderer_memory
            .lock()
            .expect("renderer memory usage poisoned") = crate::memory::CacheUsage::default();
        window.session.suspend_rendering();
        #[cfg(feature = "images")]
        {
            crate::assets::update_image_reachability(window.memory_instance, &[]);
            window.image_reachability_scene = None;
        }
        window.rendering_suspended = true;
        update_session_memory_usage(window);
        Some(window.context.memory().clone())
    });
    let Some(memory) = memory else {
        return false;
    };
    memory.notify(crate::memory::MemoryEvent::WindowHidden);
    true
}

pub(super) fn update_session_memory_usage(window: &WindowState) {
    let (component, host_scene) = window.session.memory_usage();
    *window
        .component_memory
        .lock()
        .expect("component memory usage poisoned") = component;
    *window
        .host_scene_memory
        .lock()
        .expect("host scene memory usage poisoned") = host_scene;
}

fn should_trim_host_scene(request: crate::memory::TrimRequest) -> bool {
    request.scope == crate::memory::CacheScope::AllRebuildable
        || matches!(
            request.reason,
            crate::memory::TrimReason::WindowHidden
                | crate::memory::TrimReason::AllWindowsHidden
                | crate::memory::TrimReason::SessionUnmounted
                | crate::memory::TrimReason::CriticalPressure
                | crate::memory::TrimReason::Explicit
                | crate::memory::TrimReason::Shutdown
        )
}

pub(super) fn resume_window_rendering(hwnd: HWND) {
    STATE.with(|state| {
        if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            window.rendering_suspended = false;
        }
    });
}

pub(super) fn suspend_application_if_backgrounded() {
    if !application_is_backgrounded() {
        return;
    }

    let windows = STATE.with(|state| state.borrow().keys().copied().collect::<Vec<_>>());
    for raw in windows {
        suspend_window_rendering(HWND(raw as _), true);
    }
    let context = STATE.with(|state| {
        state
            .borrow()
            .values()
            .find(|window| window.owner.is_none())
            .map(|window| window.context.clone())
    });
    if let Some(context) = context {
        context
            .memory()
            .notify(crate::memory::MemoryEvent::AllWindowsHidden);
    }
    super::super::super::background::trim_process_working_set();
    schedule_background_retrim();
}

pub(super) fn application_is_backgrounded() -> bool {
    STATE.with(|state| {
        let windows = state.borrow();
        let mut has_opted_in_root = false;
        for window in windows.values().filter(|window| window.owner.is_none()) {
            if window.visibility.desired_visible {
                return false;
            }
            has_opted_in_root |= window.background_memory_optimization;
        }
        has_opted_in_root
    })
}

pub(super) fn schedule_background_retrim() {
    let generation = BACKGROUND_TRIM_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let dispatcher = STATE.with(|state| {
        state
            .borrow()
            .values()
            .find(|window| window.owner.is_none())
            .map(|window| window.dispatcher.clone())
    });
    let Some(dispatcher) = dispatcher else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("lgui-background-trim".to_owned())
        .spawn(move || {
            std::thread::sleep(BACKGROUND_RETRIM_DELAY);
            dispatcher.post(move || {
                if BACKGROUND_TRIM_GENERATION.load(Ordering::Acquire) == generation
                    && application_is_backgrounded()
                {
                    super::super::super::background::trim_process_working_set();
                }
            });
        });
}

pub(super) fn hide_owned_windows(owner: HWND) {
    let owned = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state
            .iter_mut()
            .filter_map(|(raw, window)| {
                (window.owner == Some(owner) && window.visibility.hide_for_owner())
                    .then_some(HWND(*raw as _))
            })
            .collect::<Vec<_>>()
    });
    for hwnd in owned {
        unsafe {
            let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
        }
        suspend_window_rendering(hwnd, false);
    }
}

pub(super) fn restore_owned_windows(owner: HWND) {
    let owned = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state
            .iter_mut()
            .filter_map(|(raw, window)| {
                (window.owner == Some(owner) && window.visibility.restore_for_owner())
                    .then_some(HWND(*raw as _))
            })
            .collect::<Vec<_>>()
    });
    for hwnd in owned {
        position_window(hwnd);
        resume_window_rendering(hwnd);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

pub(super) fn reposition_owned_windows(owner: HWND) {
    let owned = STATE.with(|state| {
        state
            .borrow()
            .iter()
            .filter_map(|(raw, window)| {
                (window.owner == Some(owner) && window.visibility.desired_visible)
                    .then_some(HWND(*raw as _))
            })
            .collect::<Vec<_>>()
    });
    for hwnd in owned {
        position_window(hwnd);
    }
}

pub(super) fn position_window(hwnd: HWND) {
    let Some((owner, position, logical_size, native_titlebar)) = STATE.with(|state| {
        state.borrow().get(&(hwnd.0 as isize)).map(|state| {
            (
                state.owner,
                state.position,
                state.logical_size,
                state.native_titlebar,
            )
        })
    }) else {
        return;
    };
    let dpi = DpiContext::for_window(hwnd, logical_size);
    let size = dpi.physical_window_size;
    let mut cursor = POINT::default();
    let _ = unsafe { GetCursorPos(&mut cursor) };
    let origin = match (position, owner) {
        (WindowPosition::Absolute { x, y }, _) => PhysicalPoint::new(x, y),
        (WindowPosition::AdjacentToOwner { gap }, Some(owner)) => {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(owner, &mut rect) }.is_ok() {
                PhysicalPoint::new(rect.right + gap, rect.top)
            } else {
                PhysicalPoint::new(cursor.x + gap, cursor.y + gap)
            }
        }
        (WindowPosition::NearCursor { gap }, _) => {
            PhysicalPoint::new(cursor.x + gap, cursor.y + gap)
        }
        (WindowPosition::AdjacentToOwner { gap }, None) => {
            PhysicalPoint::new(cursor.x + gap, cursor.y + gap)
        }
        (WindowPosition::Centered, Some(owner)) => {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(owner, &mut rect) }.is_ok() {
                PhysicalPoint::new(
                    rect.left + ((rect.right - rect.left) - size.width) / 2,
                    rect.top + ((rect.bottom - rect.top) - size.height) / 2,
                )
            } else {
                PhysicalPoint::new(cursor.x - size.width / 2, cursor.y - size.height / 2)
            }
        }
        (WindowPosition::Centered, None) => PhysicalPoint::new(
            dpi.work_area.rect.left + (dpi.work_area.rect.width() - size.width) / 2,
            dpi.work_area.rect.top + (dpi.work_area.rect.height() - size.height) / 2,
        ),
    };
    let origin = dpi.clamp_origin(origin, size);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            origin.x,
            origin.y,
            size.width,
            size.height,
            SWP_NOACTIVATE
                | SWP_NOOWNERZORDER
                | SWP_NOZORDER
                | if native_titlebar {
                    windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(0)
                } else {
                    SWP_FRAMECHANGED
                },
        );
    }
}

pub(super) fn install_wake(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    STATE.with(|state| {
        if let Some(state) = state.borrow().get(&raw) {
            state.session.set_wake(Arc::new(move || unsafe {
                let _ = PostMessageW(Some(HWND(raw as _)), WM_LGUI_DISPATCH, WPARAM(0), LPARAM(0));
            }));
        }
    });
}

pub(super) fn custom_frame_hit_test(hwnd: HWND, lparam: LPARAM) -> Option<LRESULT> {
    let screen_point = unpack_point(lparam);
    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
        return Some(LRESULT(HTCLIENT as isize));
    }
    let mut client_point = POINT {
        x: screen_point.x,
        y: screen_point.y,
    };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut client_point);
    }

    STATE.with(|windows| {
        let windows = windows.borrow();
        let state = windows.get(&(hwnd.0 as isize))?;
        if state.native_titlebar {
            return None;
        }

        if state.resizable && !unsafe { IsZoomed(hwnd) }.as_bool() {
            let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
            let horizontal = unsafe {
                GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi)
                    + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
            }
            .max(1);
            let vertical = unsafe {
                GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                    + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
            }
            .max(1);
            let hit = resize_border_hit(window_rect, screen_point, horizontal, vertical);
            if hit != HTCLIENT {
                return Some(LRESULT(hit as isize));
            }
        }

        let dpi = DpiContext::for_window(hwnd, state.logical_size);
        let logical_point = dpi
            .scale
            .logical_point(PhysicalPoint::new(client_point.x, client_point.y));
        if let Some(hit) = state.session.tree().hit_test(logical_point) {
            return Some(LRESULT(
                if hit.interaction == crate::core::InteractionRole::WindowDragRegion {
                    HTCAPTION as isize
                } else {
                    HTCLIENT as isize
                },
            ));
        }

        if let Some(height) = state.titlebar_drag_height {
            let mut client = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut client) };
            let viewport = dpi
                .scale
                .logical_size(PhysicalSize::new(client.right.max(1), client.bottom.max(1)));
            let excluded = state
                .drag_exclusion
                .map(|exclusion| exclusion(viewport.width, viewport.height))
                .is_some_and(|rect| rect.contains(logical_point));
            if logical_point.y >= 0.0 && logical_point.y < height && !excluded {
                return Some(LRESULT(HTCAPTION as isize));
            }
        }

        Some(LRESULT(HTCLIENT as isize))
    })
}

pub(super) fn resize_border_hit(
    rect: RECT,
    point: PhysicalPoint,
    horizontal: i32,
    vertical: i32,
) -> u32 {
    let left = point.x >= rect.left && point.x < rect.left + horizontal;
    let right = point.x < rect.right && point.x >= rect.right - horizontal;
    let top = point.y >= rect.top && point.y < rect.top + vertical;
    let bottom = point.y < rect.bottom && point.y >= rect.bottom - vertical;

    match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT,
        (_, true, true, _) => HTTOPRIGHT,
        (true, _, _, true) => HTBOTTOMLEFT,
        (_, true, _, true) => HTBOTTOMRIGHT,
        (true, _, _, _) => HTLEFT,
        (_, true, _, _) => HTRIGHT,
        (_, _, true, _) => HTTOP,
        (_, _, _, true) => HTBOTTOM,
        _ => HTCLIENT,
    }
}
