const WM_MOUSE_LEAVE: u32 = 0x02A3;
#[cfg(feature = "renderer-gdi")]
use super::super::GdiRenderer;
#[cfg(feature = "tray")]
use super::super::Win32TrayHost;
use super::super::{
    dispatcher::{DEFAULT_FRAME_INTERVAL_MS, WM_LGUI_DISPATCH, WM_LGUI_FRAME_TICK},
    set_scale_preference, DpiContext, Win32Dispatcher,
};

const WINDOW_CLASS: &str = "LguiApplicationWindow";
const BACKGROUND_RETRIM_DELAY: Duration = Duration::from_secs(3);
const INTERACTIVE_RESIZE_FRAME_INTERVAL_MS: u64 = 33;
static BACKGROUND_TRIM_GENERATION: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static STATE: RefCell<HashMap<isize, WindowState>> = RefCell::new(HashMap::new());
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
    fn new(hwnd: HWND, hdc: HDC) -> Self {
        Self { hwnd, hdc }
    }

    pub(crate) fn hwnd(self) -> HWND {
        self.hwnd
    }

    pub(crate) fn hdc(self) -> HDC {
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

    fn text_system(&self) -> crate::text::TextSystemHandle {
        super::super::portable_text_system_handle()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GdiRendererFactory;

#[cfg(feature = "renderer-gdi")]
impl Win32RendererFactory for GdiRendererFactory {
    fn name(&self) -> &'static str {
        "gdi"
    }

    fn create(&self, _hwnd: HWND) -> Result<Box<Win32SceneRenderer>> {
        Ok(Box::new(GdiRenderer::default()))
    }
}

#[cfg(feature = "renderer-gdi")]
impl SceneRenderer for GdiRenderer {
    type Target = Win32RenderTarget;
    type Error = Win32RenderError;

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            partial_redraw: true,
            retained_surface: true,
        }
    }

    fn render(
        &mut self,
        target: &mut Self::Target,
        scene: &crate::core::Scene,
        frame: &FrameInfo<'_>,
    ) -> std::result::Result<RenderStats, Self::Error> {
        let damage = frame.damage();
        if damage.is_empty() {
            // Exposure paints reuse the retained surface and let BeginPaint's native clip limit
            // the copy to the region that Windows actually requested.
            self.draw_retained(target.hdc(), scene, frame.viewport(), damage)?;
            return Ok(RenderStats::for_frame(frame));
        }
        // BeginPaint clips its HDC to the update region that existed before rendering. Host diff
        // can discover new damage outside that region (for example, the new bounds of a moved
        // node), so present through an unclipped client DC after the retained commit is known.
        let window_target = unsafe { GetDC(Some(target.hwnd())) };
        if window_target.is_invalid() {
            self.draw_retained(target.hdc(), scene, frame.viewport(), damage)?;
            return Ok(RenderStats::for_frame(frame));
        }
        let presented = self.draw_retained(window_target, scene, frame.viewport(), damage);
        unsafe {
            let _ = ReleaseDC(Some(target.hwnd()), window_target);
        }
        presented?;
        Ok(RenderStats::for_frame(frame))
    }
}
