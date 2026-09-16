use super::*;

#[cfg(feature = "store")]
use lgui_store::StoreApplicationExt as _;

#[derive(Debug)]
pub struct WinitApplicationError(pub(super) String);

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
    pub(super) preference: GraphicsPreference,
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
        lgui_core::backend::install_window_commands(&context.windows(), {
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
            .try_resource::<lgui_core::assets::ImageCacheHandle>()
            .is_none()
        {
            let loader = context
                .try_resource::<lgui_core::assets::RemoteImageLoaderHandle>()
                .map(|loader| (*loader).clone())
                .unwrap_or_else(lgui_core::assets::http_image_loader);
            let image_wake = application_handle.clone();
            let budget = context
                .memory()
                .options()
                .domain_budget(lgui_core::memory::CacheDomain::EncodedImage);
            let cache = lgui_core::backend::async_image_cache(
                loader,
                move || image_wake.request_frame(),
                budget,
                context.memory().clone(),
            );
            let stats_cache = cache.clone();
            let trim_cache = cache.clone();
            let registration =
                context
                    .memory()
                    .register(lgui_core::memory::DomainRegistration::new(
                        lgui_core::memory::CacheDomain::EncodedImage,
                        context.memory().next_instance_id(),
                        "application:winit-images",
                        lgui_core::memory::CacheAdapter::managed(
                            move || {
                                let stats = stats_cache.stats();
                                lgui_core::memory::CacheUsage {
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
                                lgui_core::memory::TrimResult {
                                    before_bytes: before,
                                    after_bytes: trim_cache.stats().resident_bytes,
                                }
                            },
                            {
                                let cache = cache.clone();
                                move |budget| cache.set_budget(budget)
                            },
                        ),
                    ));
            lgui_core::backend::retain_memory_registration(&context, registration);
            context.resources().provide(cache);
        }
        #[cfg(feature = "store")]
        context.stores().set_wake({
            let proxy = proxy.clone();
            Arc::new(move || {
                let _ = proxy.send_event(WinitUserEvent::RequestAllFrames);
            })
        });
        #[cfg(all(feature = "notifications-win32", target_os = "windows"))]
        lgui_platform_win32::install_notification_service(&context)
            .map_err(|error| WinitApplicationError(error.to_string()))?;
        #[cfg(feature = "renderer-skia")]
        let _text_environment = lgui_core::backend::install_text_environment(
            &context,
            lgui_render_skia::skia_text_system_handle(),
        );

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
            #[cfg(all(feature = "tray-win32", target_os = "windows"))]
            tray_host: None,
            exit_requested: false,
            backgrounded: false,
        };
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}
