use super::*;

impl ApplicationBackend for Win32Application {
    type Error = Error;

    fn run(self, options: WindowOptions, view: AppView, context: ApplicationContext) -> Result<()> {
        let win32_options = options
            .platform_options::<Win32WindowOptions>()
            .cloned()
            .unwrap_or_default();
        let dispatcher = Win32Dispatcher::new();
        #[cfg(feature = "images-win32")]
        let _gdiplus = super::super::super::gdiplus::GdiPlusRuntime::start()?;
        #[cfg(feature = "images-win32")]
        let _remote_image_loader = super::super::super::install_remote_image_loader(
            context
                .try_resource::<crate::assets::RemoteImageLoaderHandle>()
                .map(|loader| (*loader).clone())
                .unwrap_or_else(crate::assets::http_image_loader),
        );
        #[cfg(feature = "images-win32")]
        let _image_memory_governor =
            super::super::super::install_image_memory_governor(context.memory().clone());
        #[cfg(feature = "images-win32")]
        let image_cache_handle = super::super::super::portable_image_cache_handle();
        #[cfg(feature = "images-win32")]
        image_cache_handle.set_budget(
            context
                .memory()
                .options()
                .domain_budget(crate::memory::CacheDomain::EncodedImage),
        );
        #[cfg(feature = "images-win32")]
        let _image_cache = crate::assets::install_image_cache(image_cache_handle.clone());
        #[cfg(feature = "images-win32")]
        {
            let stats_cache = image_cache_handle.clone();
            let trim_cache = image_cache_handle.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::EncodedImage,
                    context.memory().next_instance_id(),
                    "application:win32-images",
                    crate::memory::CacheAdapter::managed(
                        move || {
                            let stats = stats_cache.stats();
                            crate::memory::CacheUsage {
                                cache_bytes: stats.resident_bytes,
                                cpu_bytes: stats.resident_bytes,
                                pinned_bytes: stats.pinned_bytes,
                                entries: stats.entries,
                                hits: stats.hits,
                                misses: stats.misses,
                                evictions: stats.evictions,
                                largest_entry_bytes: stats.largest_entry_bytes,
                                in_flight: stats.in_flight,
                                ..Default::default()
                            }
                        },
                        move |request| {
                            let before = trim_cache.stats().resident_bytes;
                            trim_cache.trim_to(request.target_bytes);
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: trim_cache.stats().resident_bytes,
                            }
                        },
                        {
                            let cache = image_cache_handle.clone();
                            move |budget| cache.set_budget(budget)
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        #[cfg(feature = "images-win32")]
        {
            let trim_dispatcher = dispatcher.clone();
            let budget_dispatcher = dispatcher.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::DecodedImage,
                    context.memory().next_instance_id(),
                    "application:win32-gdiplus-images",
                    crate::memory::CacheAdapter::managed(
                        super::super::super::decoded_image_cache_usage,
                        move |request| {
                            let before =
                                super::super::super::decoded_image_cache_usage().resident_bytes();
                            trim_dispatcher.post(move || {
                                super::super::super::trim_decoded_image_cache(request.target_bytes);
                            });
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        move |budget| {
                            budget_dispatcher.post(move || {
                                super::super::super::set_decoded_image_cache_budget(budget);
                            });
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        #[cfg(feature = "renderer-d2d")]
        {
            let trim_dispatcher = dispatcher.clone();
            let budget_dispatcher = dispatcher.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::DecodedImage,
                    context.memory().next_instance_id(),
                    "application:win32-enhanced-images",
                    crate::memory::CacheAdapter::managed(
                        super::super::super::enhanced::image::decoded_image_cache_usage,
                        move |request| {
                            let before = super::super::super::enhanced::image::decoded_image_cache_usage()
                                .resident_bytes();
                            trim_dispatcher.post(move || {
                                super::super::super::enhanced::image::trim_decoded_image_cache(
                                    request.target_bytes,
                                );
                            });
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        move |budget| {
                            budget_dispatcher.post(move || {
                                super::super::super::enhanced::image::set_decoded_image_cache_budget(
                                    budget,
                                );
                            });
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        #[cfg(feature = "renderer-d2d")]
        {
            let trim_dispatcher = dispatcher.clone();
            let budget_dispatcher = dispatcher.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::Blur,
                    context.memory().next_instance_id(),
                    "application:win32-blur",
                    crate::memory::CacheAdapter::managed(
                        super::super::super::enhanced::blur::blur_cache_usage,
                        move |request| {
                            let before = super::super::super::enhanced::blur::blur_cache_usage()
                                .resident_bytes();
                            trim_dispatcher.post(move || {
                                super::super::super::enhanced::blur::trim_blur_caches(
                                    request.target_bytes,
                                );
                            });
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        move |budget| {
                            budget_dispatcher.post(move || {
                                super::super::super::enhanced::blur::set_blur_cache_budget(budget);
                            });
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        #[cfg(feature = "advanced-rendering")]
        {
            let trim_dispatcher = dispatcher.clone();
            let budget_dispatcher = dispatcher.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::StaticLayer,
                    context.memory().next_instance_id(),
                    "application:win32-static-layer",
                    crate::memory::CacheAdapter::managed(
                        || {
                            let stats = super::super::super::enhanced::static_layer::static_layer_memory_cache_stats();
                            crate::memory::CacheUsage {
                                cache_bytes: stats.bytes,
                                cpu_bytes: stats.bytes,
                                entries: stats.entry_count,
                                hits: stats.hits,
                                misses: stats.misses,
                                evictions: stats.evictions,
                                ..Default::default()
                            }
                        },
                        move |request| {
                            let before = super::super::super::enhanced::static_layer::static_layer_memory_cache_stats().bytes;
                            trim_dispatcher.post(move || {
                                super::super::super::enhanced::static_layer::trim_static_layer_memory_cache(request.target_bytes);
                            });
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        move |budget| {
                            budget_dispatcher.post(move || {
                                super::super::super::enhanced::static_layer::set_static_layer_memory_cache_budget(budget);
                            });
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        #[cfg(feature = "advanced-rendering")]
        {
            let trim_dispatcher = dispatcher.clone();
            let budget_dispatcher = dispatcher.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::Gdi,
                    context.memory().next_instance_id(),
                    "application:win32-gdi-shared",
                    crate::memory::CacheAdapter::managed(
                        super::super::super::enhanced::gdi_renderer_cache_usage,
                        move |request| {
                            let before = super::super::super::enhanced::gdi_renderer_cache_usage()
                                .resident_bytes();
                            trim_dispatcher.post(move || {
                                super::super::super::enhanced::trim_gdi_renderer_caches(
                                    request.target_bytes,
                                );
                            });
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        move |budget| {
                            budget_dispatcher.post(move || {
                                super::super::super::enhanced::set_gdi_renderer_cache_budget(
                                    budget,
                                );
                            });
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        #[cfg(feature = "advanced-rendering")]
        let _render_cache = crate::renderer::install_render_cache(
            super::super::super::enhanced::portable_render_cache_handle(),
        );
        if let Some(fonts) = context.try_resource::<crate::text::FontFamilies>() {
            super::super::super::set_ui_font_families(fonts.0);
        }
        let font_families = context
            .try_resource::<crate::text::FontFamilies>()
            .map_or(&["Segoe UI"][..], |families| families.0);
        let _font_families = crate::text::install_font_families(font_families);
        let _text_system = crate::text::install_text_system(self.renderer_factory.text_system());
        #[cfg(feature = "svg")]
        if let Some(registration) = context.try_resource::<crate::icons::IconRegistration>() {
            let _ = super::super::super::install_svg_icon_registry((*registration.0).clone());
        }
        #[cfg(feature = "svg")]
        {
            let trim_dispatcher = dispatcher.clone();
            let budget_dispatcher = dispatcher.clone();
            let registration = context
                .memory()
                .register(crate::memory::DomainRegistration::new(
                    crate::memory::CacheDomain::Svg,
                    context.memory().next_instance_id(),
                    "application:win32-svg",
                    crate::memory::CacheAdapter::managed(
                        super::super::super::svg::svg_bitmap_cache_usage,
                        move |request| {
                            let before =
                                super::super::super::svg::svg_bitmap_cache_usage().resident_bytes();
                            trim_dispatcher.post(move || {
                                super::super::super::svg::trim_svg_bitmap_cache(
                                    request.target_bytes,
                                );
                            });
                            crate::memory::TrimResult {
                                before_bytes: before,
                                after_bytes: before,
                            }
                        },
                        move |budget| {
                            budget_dispatcher.post(move || {
                                super::super::super::svg::set_svg_bitmap_cache_budget(budget);
                            });
                        },
                    ),
                ));
            context.retain_memory_registration(registration);
        }
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        set_scale_preference(options.scale_preference);
        #[cfg(feature = "notifications-win32")]
        super::super::super::services::install_notification_service(&context)
            .map_err(|error| Error::new(HRESULT(0x80004005_u32 as i32), error.to_string()))?;
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
        let class_name = wide(win32_options.class_name.as_deref().unwrap_or(WINDOW_CLASS));
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
        let class_icons = WindowClassIcons::from_ico_bytes(win32_options.icon_bytes)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            // WM_SIZE drives invalidation explicitly so interactive resize can be throttled.
            style: Default::default(),
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hIcon: class_icons.large(),
            hCursor: cursor,
            hIconSm: class_icons.small(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassExW(&class) } == 0 {
            return Err(Error::from_thread());
        }
        let _window_class = RegisteredWindowClass {
            instance,
            class_name: class_name.clone(),
            icons: class_icons,
        };

        context.resources().provide(dispatcher.application_handle());
        let factory = Arc::clone(&self.renderer_factory);
        let main_id = options.id.clone();
        let hwnd = create_window(
            instance,
            &class_name,
            options,
            view,
            context.clone(),
            Arc::clone(&factory),
            dispatcher.clone(),
        )?;
        dispatcher.attach(hwnd);
        #[cfg(feature = "images-win32")]
        crate::platform::win32::register_image_repaint_hwnd(hwnd);
        #[cfg(feature = "store")]
        context.stores().set_wake({
            let dispatcher = dispatcher.clone();
            Arc::new(move || dispatcher.notify())
        });
        let manager = context.windows();
        let instance_value = instance.0 as isize;
        let command_dispatcher = dispatcher.clone();
        let command_context = context.clone();
        manager.install(move |command| {
            let dispatcher = command_dispatcher.clone();
            let task_dispatcher = dispatcher.clone();
            let class_name = class_name.clone();
            let context = command_context.clone();
            let factory = Arc::clone(&factory);
            dispatcher.post(move || {
                execute_window_command(
                    command,
                    instance_value,
                    &class_name,
                    context,
                    factory,
                    task_dispatcher,
                );
            });
        });
        #[cfg(feature = "tray-win32")]
        let mut tray_host = if let Some(registration) = context.try_resource::<TrayRegistration>() {
            let visibility_dispatcher = dispatcher.clone();
            let main_hwnd = hwnd.0 as isize;
            let action_context = context.clone();
            let action_registration = Arc::clone(&registration);
            Some(
                Win32TrayHost::spawn(registration.options.clone(), move |action| {
                    dispatch_tray_action(
                        &action_registration,
                        &action_context,
                        action,
                        |visible| {
                            let dispatcher = visibility_dispatcher.clone();
                            dispatcher.post(move || {
                                if visible {
                                    show_window(HWND(main_hwnd as _));
                                    unsafe {
                                        let _ = SetForegroundWindow(HWND(main_hwnd as _));
                                    }
                                } else {
                                    hide_window(HWND(main_hwnd as _));
                                }
                            });
                        },
                    );
                })
                .map_err(|error| Error::new(HRESULT(0x80004005_u32 as i32), error.to_string()))?,
            )
        } else {
            None
        };
        suspend_application_if_backgrounded();
        debug_assert_eq!(hwnd_for_id(&main_id), Some(hwnd));

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        #[cfg(feature = "tray-win32")]
        if let Some(host) = tray_host.as_mut() {
            host.shutdown();
        }
        context
            .memory()
            .notify(crate::memory::MemoryEvent::ApplicationShutdown);
        STATE.with(|state| state.borrow_mut().clear());
        Ok(())
    }
}

pub(super) fn execute_window_command(
    command: WindowCommand,
    instance: isize,
    class_name: &[u16],
    context: ApplicationContext,
    factory: Arc<dyn Win32RendererFactory>,
    dispatcher: Win32Dispatcher,
) {
    match command {
        WindowCommand::Show { options, view } => {
            if let Some(hwnd) = hwnd_for_id(&options.id) {
                show_window(hwnd);
            } else {
                let Ok(hwnd) = create_window(
                    HINSTANCE(instance as _),
                    class_name,
                    options,
                    application_root_view(context.clone(), view),
                    context,
                    factory,
                    dispatcher,
                ) else {
                    return;
                };
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                }
            }
        }
        WindowCommand::Toggle { options, view } => {
            if let Some(hwnd) = hwnd_for_id(&options.id) {
                if unsafe { IsWindowVisible(hwnd).as_bool() } {
                    hide_window(hwnd);
                } else {
                    show_window(hwnd);
                }
            } else {
                let _ = create_window(
                    HINSTANCE(instance as _),
                    class_name,
                    options,
                    application_root_view(context.clone(), view),
                    context,
                    factory,
                    dispatcher,
                );
            }
        }
        WindowCommand::Hide(id) => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                hide_window(hwnd);
            }
        }
        WindowCommand::Close(id) => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
        WindowCommand::RequestClose(id) => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                request_window_close(hwnd);
            }
        }
        WindowCommand::Minimize(id) => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                unsafe {
                    let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE);
                }
            }
        }
        WindowCommand::SetScalePreference(preference) => {
            set_scale_preference(preference);
            context
                .memory()
                .notify(crate::memory::MemoryEvent::ThemeOrScaleChanged);
            let windows = STATE.with(|state| {
                state
                    .borrow()
                    .iter()
                    .map(|(raw, state)| (HWND(*raw as _), state.logical_size))
                    .collect::<Vec<_>>()
            });
            for (hwnd, logical_size) in windows {
                let physical = DpiContext::for_window(hwnd, logical_size).physical_window_size;
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        physical.width,
                        physical.height,
                        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER,
                    );
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
        }
        WindowCommand::SetMode { id, mode } => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                set_window_mode(hwnd, mode);
            }
        }
        WindowCommand::Input { id, input } => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                dispatch_input(hwnd, input);
            }
        }
        WindowCommand::Exit => {
            let windows = STATE.with(|state| state.borrow().keys().copied().collect::<Vec<_>>());
            for raw in windows {
                unsafe {
                    let _ = DestroyWindow(HWND(raw as _));
                }
            }
        }
    }
}
