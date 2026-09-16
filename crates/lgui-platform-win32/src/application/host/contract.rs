use super::*;

pub(super) const WM_MOUSE_LEAVE: u32 = 0x02A3;
pub(super) const WINDOW_CLASS: &str = "LguiApplicationWindow";
pub(super) const BACKGROUND_RETRIM_DELAY: Duration = Duration::from_secs(3);
pub(super) const INTERACTIVE_RESIZE_FRAME_INTERVAL_MS: u64 = 33;
pub(super) static BACKGROUND_TRIM_GENERATION: AtomicU64 = AtomicU64::new(0);

thread_local! {
    pub(super) static STATE: RefCell<HashMap<isize, WindowState>> = RefCell::new(HashMap::new());
}

#[derive(Debug)]
pub struct Win32RenderError {
    stage: RenderErrorStage,
    operation: &'static str,
    source: Error,
}

impl Win32RenderError {
    pub fn new(stage: RenderErrorStage, operation: &'static str, source: Error) -> Self {
        Self {
            stage,
            operation,
            source,
        }
    }

    pub const fn stage(&self) -> RenderErrorStage {
        self.stage
    }

    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn source_error(&self) -> &Error {
        &self.source
    }
}

impl std::fmt::Display for Win32RenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} failed during {}: {}",
            self.operation,
            self.stage.as_str(),
            self.source
        )
    }
}

impl std::error::Error for Win32RenderError {}

#[derive(Clone, Copy, Debug)]
pub struct Win32RenderTarget {
    hwnd: HWND,
    hdc: HDC,
}

impl Win32RenderTarget {
    pub(super) fn new(hwnd: HWND, hdc: HDC) -> Self {
        Self { hwnd, hdc }
    }

    pub fn hwnd(self) -> HWND {
        self.hwnd
    }

    pub fn hdc(self) -> HDC {
        self.hdc
    }
}

pub type Win32SceneRenderer =
    dyn SceneRenderer<Target = Win32RenderTarget, Error = Win32RenderError>;

pub trait Win32RendererFactory: Send + Sync + 'static {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn create(&self, hwnd: HWND) -> Result<Box<Win32SceneRenderer>>;

    fn text_system(&self) -> lgui_core::text::TextSystemHandle {
        super::super::super::portable_text_system_handle()
    }

    fn install_environment(
        &self,
        _context: &ApplicationContext,
        _dispatcher: &Win32Dispatcher,
    ) -> Box<dyn std::any::Any> {
        Box::new(())
    }

    #[cfg(feature = "diagnostics")]
    fn reset_present_metrics(&self) {}

    #[cfg(feature = "diagnostics")]
    fn take_present_metrics(&self, submitted_pixels: u64) -> FramePresentMetrics {
        FramePresentMetrics {
            submitted_pixels,
            ..FramePresentMetrics::default()
        }
    }
}
