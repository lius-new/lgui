pub struct Win32Application {
    renderer_factory: Arc<dyn Win32RendererFactory>,
}

struct OwnedIcon(HICON);

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
struct WindowClassIcons {
    large: Option<OwnedIcon>,
    small: Option<OwnedIcon>,
}

impl WindowClassIcons {
    fn from_ico_bytes(bytes: Option<&[u8]>) -> Result<Self> {
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

    fn large(&self) -> HICON {
        self.large
            .as_ref()
            .map_or_else(HICON::default, OwnedIcon::handle)
    }

    fn small(&self) -> HICON {
        self.small
            .as_ref()
            .map_or_else(HICON::default, OwnedIcon::handle)
    }
}

struct RegisteredWindowClass {
    instance: HINSTANCE,
    class_name: Vec<u16>,
    icons: WindowClassIcons,
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

struct WindowState {
    id: WindowId,
    view: AppView,
    context: ApplicationContext,
    session: UiSession,
    renderer: Option<Box<Win32SceneRenderer>>,
    renderer_factory: Arc<dyn Win32RendererFactory>,
    logical_size: Size,
    minimum_size: Option<Size>,
    maximum_size: Option<Size>,
    resizable: bool,
    native_titlebar: bool,
    corner_radius: i32,
    titlebar_drag_height: Option<f32>,
    drag_exclusion: Option<WindowDragExclusion>,
    windowed_style: WINDOW_STYLE,
    windowed_placement: Option<WINDOWPLACEMENT>,
    mode: WindowMode,
    owner: Option<HWND>,
    position: WindowPosition,
    hide_on_deactivate: bool,
    background_memory_optimization: bool,
    rendering_suspended: bool,
    interaction_mode: WindowInteractionMode,
    resize_frame_throttle: ResizeFrameThrottle,
    visibility: OwnerVisibility,
    close_policy: ClosePolicy,
    close_handler: Option<WindowCloseHandler>,
    dispatcher: Win32Dispatcher,
    #[cfg(feature = "diagnostics")]
    diagnostics: Option<Arc<DiagnosticsRegistration>>,
    render_retry_used: bool,
    #[cfg(feature = "diagnostics")]
    frame_index: u64,
    suppressed_ime_char_units: VecDeque<u16>,
    pending_high_surrogate: Option<u16>,
    pointer_inside: bool,
}

impl WindowState {
    fn can_advance_animations(&self) -> bool {
        can_advance_window_animations(
            self.rendering_suspended,
            self.visibility,
            self.interaction_mode,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WindowInteractionMode {
    #[default]
    Idle,
    MoveResize,
    Moving,
    Sizing,
}

fn can_advance_window_animations(
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
struct ResizeFrameThrottle {
    pending: bool,
    elapsed_ms: f32,
}

impl ResizeFrameThrottle {
    fn begin(&mut self) {
        self.pending = false;
        self.elapsed_ms = INTERACTIVE_RESIZE_FRAME_INTERVAL_MS as f32;
    }

    fn request(&mut self) {
        self.pending = true;
    }

    fn advance(&mut self, elapsed_ms: f32) -> bool {
        self.elapsed_ms = (self.elapsed_ms + elapsed_ms.max(0.0))
            .min(INTERACTIVE_RESIZE_FRAME_INTERVAL_MS as f32);
        if !self.pending || self.elapsed_ms < INTERACTIVE_RESIZE_FRAME_INTERVAL_MS as f32 {
            return false;
        }
        self.pending = false;
        self.elapsed_ms = 0.0;
        true
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnerVisibility {
    desired_visible: bool,
    hidden_for_owner: bool,
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
    fn visible() -> Self {
        Self {
            desired_visible: true,
            hidden_for_owner: false,
        }
    }

    fn set_desired(&mut self, visible: bool) {
        self.desired_visible = visible;
        if !visible {
            self.hidden_for_owner = false;
        }
    }

    fn hide_for_owner(&mut self) -> bool {
        if !self.desired_visible {
            return false;
        }
        self.hidden_for_owner = true;
        true
    }

    fn restore_for_owner(&mut self) -> bool {
        if !self.desired_visible || !self.hidden_for_owner {
            return false;
        }
        self.hidden_for_owner = false;
        true
    }
}
