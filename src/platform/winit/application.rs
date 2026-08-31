use super::*;

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
