use super::*;

pub struct Win32Application {
    pub(super) renderer_factory: Arc<dyn Win32RendererFactory>,
}

pub(super) struct OwnedIcon(HICON);

impl OwnedIcon {
    fn from_ico_bytes(bytes: &[u8], width: i32, height: i32) -> Result<Self> {
        create_icon_from_ico_bytes(bytes, width, height)
            .map(Self)
            .map_err(|error| Error::new(HRESULT(0x80004005_u32 as i32), error.to_string()))
    }

    const fn handle(&self) -> HICON {
        self.0
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

#[derive(Default)]
pub(super) struct WindowClassIcons {
    pub(super) large: Option<OwnedIcon>,
    pub(super) small: Option<OwnedIcon>,
}

impl WindowClassIcons {
    pub(super) fn from_ico_bytes(bytes: Option<&[u8]>) -> Result<Self> {
        let Some(bytes) = bytes else {
            return Ok(Self::default());
        };
        let large =
            OwnedIcon::from_ico_bytes(bytes, unsafe { GetSystemMetrics(SM_CXICON) }, unsafe {
                GetSystemMetrics(SM_CYICON)
            })?;
        let small =
            OwnedIcon::from_ico_bytes(bytes, unsafe { GetSystemMetrics(SM_CXSMICON) }, unsafe {
                GetSystemMetrics(SM_CYSMICON)
            })?;
        Ok(Self {
            large: Some(large),
            small: Some(small),
        })
    }

    pub(super) fn large(&self) -> HICON {
        self.large
            .as_ref()
            .map_or_else(HICON::default, OwnedIcon::handle)
    }

    pub(super) fn small(&self) -> HICON {
        self.small
            .as_ref()
            .map_or_else(HICON::default, OwnedIcon::handle)
    }
}

pub(super) struct RegisteredWindowClass {
    pub(super) instance: HINSTANCE,
    pub(super) class_name: Vec<u16>,
    pub(super) icons: WindowClassIcons,
}

impl Drop for RegisteredWindowClass {
    fn drop(&mut self) {
        if unsafe { UnregisterClassW(PCWSTR(self.class_name.as_ptr()), Some(self.instance)) }
            .is_err()
        {
            // A surviving class/window can still dereference these handles. Keep them valid until
            // process exit rather than destroying resources that Windows continues to own.
            std::mem::forget(std::mem::take(&mut self.icons));
        }
    }
}

impl Win32Application {
    pub fn with_renderer(factory: impl Win32RendererFactory) -> Self {
        Self {
            renderer_factory: Arc::new(factory),
        }
    }
}

#[cfg(feature = "renderer-gdi")]
impl Default for Win32Application {
    fn default() -> Self {
        Self::with_renderer(GdiRendererFactory)
    }
}

pub(super) struct WindowState {
    pub(super) id: WindowId,
    pub(super) view: AppView,
    pub(super) context: ApplicationContext,
    pub(super) session: UiSession,
    pub(super) renderer: Option<Box<Win32SceneRenderer>>,
    pub(super) renderer_memory: Arc<Mutex<crate::memory::CacheUsage>>,
    pub(super) renderer_budget: Arc<AtomicUsize>,
    #[cfg(feature = "images-win32")]
    pub(super) memory_instance: crate::memory::DomainInstanceId,
    #[cfg(feature = "images-win32")]
    pub(super) image_reachability_scene: Option<crate::core::Scene>,
    pub(super) _renderer_memory_registration: crate::memory::CacheRegistration,
    pub(super) component_memory: Arc<Mutex<crate::memory::CacheUsage>>,
    pub(super) host_scene_memory: Arc<Mutex<crate::memory::CacheUsage>>,
    pub(super) _component_memory_registration: crate::memory::CacheRegistration,
    pub(super) _host_scene_memory_registration: crate::memory::CacheRegistration,
    pub(super) renderer_factory: Arc<dyn Win32RendererFactory>,
    pub(super) logical_size: Size,
    pub(super) minimum_size: Option<Size>,
    pub(super) maximum_size: Option<Size>,
    pub(super) resizable: bool,
    pub(super) native_titlebar: bool,
    pub(super) corner_radius: i32,
    pub(super) titlebar_drag_height: Option<f32>,
    pub(super) drag_exclusion: Option<WindowDragExclusion>,
    pub(super) windowed_style: WINDOW_STYLE,
    pub(super) windowed_placement: Option<WINDOWPLACEMENT>,
    pub(super) mode: WindowMode,
    pub(super) owner: Option<HWND>,
    pub(super) position: WindowPosition,
    pub(super) hide_on_deactivate: bool,
    pub(super) background_memory_optimization: bool,
    pub(super) rendering_suspended: bool,
    pub(super) interaction_mode: WindowInteractionMode,
    pub(super) resize_frame_throttle: ResizeFrameThrottle,
    pub(super) visibility: OwnerVisibility,
    pub(super) close_policy: ClosePolicy,
    pub(super) close_handler: Option<WindowCloseHandler>,
    pub(super) dispatcher: Win32Dispatcher,
    #[cfg(feature = "diagnostics")]
    pub(super) diagnostics: Option<Arc<DiagnosticsRegistration>>,
    pub(super) render_retry_used: bool,
    #[cfg(feature = "diagnostics")]
    pub(super) frame_index: u64,
    pub(super) suppressed_ime_char_units: VecDeque<u16>,
    pub(super) pending_high_surrogate: Option<u16>,
    pub(super) pointer_inside: bool,
}

impl WindowState {
    pub(super) fn can_advance_animations(&self) -> bool {
        can_advance_window_animations(
            self.rendering_suspended,
            self.visibility,
            self.interaction_mode,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WindowInteractionMode {
    #[default]
    Idle,
    MoveResize,
    Moving,
    Sizing,
}

pub(super) fn can_advance_window_animations(
    rendering_suspended: bool,
    visibility: OwnerVisibility,
    interaction_mode: WindowInteractionMode,
) -> bool {
    !rendering_suspended
        && visibility.desired_visible
        && !visibility.hidden_for_owner
        && interaction_mode == WindowInteractionMode::Idle
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ResizeFrameThrottle {
    pub(super) pending: bool,
    pub(super) elapsed_ms: f32,
}

impl ResizeFrameThrottle {
    pub(super) fn begin(&mut self) {
        self.pending = false;
        self.elapsed_ms = INTERACTIVE_RESIZE_FRAME_INTERVAL_MS as f32;
    }

    pub(super) fn request(&mut self) {
        self.pending = true;
    }

    pub(super) fn advance(&mut self, elapsed_ms: f32) -> bool {
        self.elapsed_ms = (self.elapsed_ms + elapsed_ms.max(0.0))
            .min(INTERACTIVE_RESIZE_FRAME_INTERVAL_MS as f32);
        if !self.pending || self.elapsed_ms < INTERACTIVE_RESIZE_FRAME_INTERVAL_MS as f32 {
            return false;
        }
        self.pending = false;
        self.elapsed_ms = 0.0;
        true
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnerVisibility {
    pub(super) desired_visible: bool,
    pub(super) hidden_for_owner: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Win32WindowOptions {
    pub class_name: Option<String>,
    pub icon_bytes: Option<&'static [u8]>,
}

impl Win32WindowOptions {
    pub fn class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = Some(class_name.into());
        self
    }

    pub fn icon_bytes(mut self, bytes: &'static [u8]) -> Self {
        self.icon_bytes = Some(bytes);
        self
    }
}

impl Default for Win32WindowOptions {
    fn default() -> Self {
        Self {
            class_name: None,
            icon_bytes: None,
        }
    }
}

impl OwnerVisibility {
    #[cfg(test)]
    pub(super) fn visible() -> Self {
        Self {
            desired_visible: true,
            hidden_for_owner: false,
        }
    }

    pub(super) fn set_desired(&mut self, visible: bool) {
        self.desired_visible = visible;
        if !visible {
            self.hidden_for_owner = false;
        }
    }

    pub(super) fn hide_for_owner(&mut self) -> bool {
        if !self.desired_visible {
            return false;
        }
        self.hidden_for_owner = true;
        true
    }

    pub(super) fn restore_for_owner(&mut self) -> bool {
        if !self.desired_visible || !self.hidden_for_owner {
            return false;
        }
        self.hidden_for_owner = false;
        true
    }
}
