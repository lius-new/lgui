use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    mem::size_of,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use windows::{
    core::{Error, Result, HRESULT, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
                DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE,
            },
            Gdi::{
                BeginPaint, EndPaint, GetDC, GetMonitorInfoW, InvalidateRect, MonitorFromWindow,
                ReleaseDC, ScreenToClient, UpdateWindow, HDC, MONITORINFO,
                MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{
                GetDpiForWindow, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
            Input::{
                Ime::{
                    ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
                    ImmSetCompositionWindow, CFS_POINT, COMPOSITIONFORM, GCS_COMPSTR,
                    GCS_RESULTSTR,
                },
                KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT},
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyCaret, DestroyIcon, DestroyWindow,
                DispatchMessageW, GetClassLongPtrW, GetClientRect, GetCursorPos, GetMessageW,
                GetSystemMetrics, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
                IsWindowVisible, IsZoomed, LoadCursorW, PostMessageW, PostQuitMessage,
                RegisterClassExW, SetLayeredWindowAttributes, SetWindowLongPtrW,
                SetWindowPlacement, SetWindowPos, ShowWindow, TranslateMessage, UnregisterClassW,
                CW_USEDEFAULT, GCLP_HICON, GCLP_HICONSM, GWL_STYLE, HICON, HTBOTTOM, HTBOTTOMLEFT,
                HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
                ICON_BIG, ICON_SMALL, IDC_ARROW, LWA_ALPHA, MINMAXINFO, MSG, SIZE_MINIMIZED,
                SM_CXICON, SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CXSMICON, SM_CYICON,
                SM_CYSIZEFRAME, SM_CYSMICON, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
                SWP_NOZORDER, SW_HIDE, SW_SHOW, WA_INACTIVE, WINDOWPLACEMENT, WINDOW_EX_STYLE,
                WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
                WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
                WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_MOVING,
                WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_SETICON, WM_SIZE, WM_SIZING, WNDCLASSEXW,
                WS_CAPTION, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MAXIMIZEBOX,
                WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
            },
        },
    },
};

#[cfg(feature = "notifications")]
use crate::application::NotificationRegistration;
#[cfg(feature = "tray")]
use crate::application::TrayRegistration;
#[cfg(feature = "notifications")]
use crate::platform::{NotificationError, NotificationHandle};
use crate::{
    application::{
        application_root_view, AppView, ApplicationBackend, ApplicationContext, ClosePolicy,
        RenderError, RenderErrorStage, WindowCloseHandler, WindowCommand, WindowDragExclusion,
        WindowId, WindowMode, WindowOptions, WindowPosition,
    },
    core::{
        dispatch_runtime_output, InputEvent, KeyCode, KeyModifiers, Point, PointerButton,
        RuntimeOutput, Size, UiEvent, UiRect,
    },
    session::UiSession,
};

use super::ico::create_icon_from_ico_bytes;
#[cfg(feature = "renderer-gdi")]
use super::GdiRenderer;
#[cfg(feature = "tray")]
use super::Win32TrayHost;
use super::{
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

pub trait Win32Renderer: 'static {
    fn draw(
        &mut self,
        hwnd: HWND,
        target: HDC,
        scene: &crate::core::Scene,
        viewport: UiRect,
    ) -> std::result::Result<(), Win32RenderError>;

    /// Draws a retained scene using physical-pixel damage rectangles.
    ///
    /// Backends that cannot safely preserve previous pixels may keep the default full-draw
    /// behavior. The platform still avoids invoking them for input that produced no damage.
    fn draw_damage(
        &mut self,
        hwnd: HWND,
        target: HDC,
        scene: &crate::core::Scene,
        viewport: UiRect,
        _damage: &[UiRect],
    ) -> std::result::Result<(), Win32RenderError> {
        self.draw(hwnd, target, scene, viewport)
    }
}

pub trait Win32RendererFactory: Send + Sync + 'static {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn create(&self, hwnd: HWND) -> Result<Box<dyn Win32Renderer>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GdiRendererFactory;

#[cfg(feature = "renderer-gdi")]
impl Win32RendererFactory for GdiRendererFactory {
    fn name(&self) -> &'static str {
        "gdi"
    }

    fn create(&self, _hwnd: HWND) -> Result<Box<dyn Win32Renderer>> {
        Ok(Box::new(GdiRenderer::default()))
    }
}

#[cfg(feature = "renderer-gdi")]
impl Win32Renderer for GdiRenderer {
    fn draw(
        &mut self,
        _hwnd: HWND,
        target: HDC,
        scene: &crate::core::Scene,
        viewport: UiRect,
    ) -> std::result::Result<(), Win32RenderError> {
        self.draw_retained(target, scene, viewport, &[viewport])
    }

    fn draw_damage(
        &mut self,
        hwnd: HWND,
        target: HDC,
        scene: &crate::core::Scene,
        viewport: UiRect,
        damage: &[UiRect],
    ) -> std::result::Result<(), Win32RenderError> {
        if damage.is_empty() {
            // Exposure paints reuse the retained surface and let BeginPaint's native clip limit
            // the copy to the region that Windows actually requested.
            return self.draw_retained(target, scene, viewport, damage);
        }
        // BeginPaint clips its HDC to the update region that existed before rendering. Host diff
        // can discover new damage outside that region (for example, the new bounds of a moved
        // node), so present through an unclipped client DC after the retained commit is known.
        let window_target = unsafe { GetDC(Some(hwnd)) };
        if window_target.is_invalid() {
            return self.draw_retained(target, scene, viewport, damage);
        }
        let presented = self.draw_retained(window_target, scene, viewport, damage);
        unsafe {
            let _ = ReleaseDC(Some(hwnd), window_target);
        }
        presented
    }
}

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
    renderer: Option<Box<dyn Win32Renderer>>,
    renderer_factory: Arc<dyn Win32RendererFactory>,
    logical_size: Size,
    minimum_size: Option<Size>,
    maximum_size: Option<Size>,
    resizable: bool,
    native_titlebar: bool,
    rounded_corners: bool,
    titlebar_drag_height: Option<i32>,
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
    render_retry_used: bool,
    suppressed_ime_char_units: VecDeque<u16>,
    pending_high_surrogate: Option<u16>,
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

impl ApplicationBackend for Win32Application {
    type Error = Error;

    fn run(self, options: WindowOptions, view: AppView, context: ApplicationContext) -> Result<()> {
        #[cfg(feature = "images")]
        let _gdiplus = super::gdiplus::GdiPlusRuntime::start()?;
        if let Some(fonts) = context.try_resource::<crate::text::FontFamilies>() {
            super::set_ui_font_families(fonts.0);
        }
        #[cfg(feature = "svg")]
        if let Some(registration) = context.try_resource::<crate::icons::IconRegistration>() {
            if let Some(registry) = registration
                .0
                .lock()
                .expect("SVG icon registration poisoned")
                .take()
            {
                let _ = super::install_svg_icon_registry(registry);
            }
        }
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        set_scale_preference(options.scale_preference);
        #[cfg(feature = "notifications")]
        let notification_registration = context.try_resource::<NotificationRegistration>();
        #[cfg(feature = "notifications")]
        if let Some(registration) = notification_registration.as_ref() {
            super::notifications::initialize_process_identity(&registration.identity)
                .map_err(|error| Error::new(HRESULT(0x80004005_u32 as i32), error.to_string()))?;
        }
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
        let class_name = wide(options.class_name.as_deref().unwrap_or(WINDOW_CLASS));
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
        let class_icons = WindowClassIcons::from_ico_bytes(options.icon_bytes)?;
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

        let dispatcher = Win32Dispatcher::new();
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
        #[cfg(feature = "notifications")]
        if let Some(registration) = notification_registration {
            let service = super::Win32NotificationService::new(&registration.identity)
                .map_err(|error| Error::new(HRESULT(0x80004005_u32 as i32), error.to_string()))?;
            context
                .resources()
                .provide(NotificationHandle::new(move |notification| {
                    service
                        .show(&notification.title, &notification.body)
                        .map_err(|error| NotificationError::new(error.to_string()))
                }));
        }
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
        #[cfg(feature = "tray")]
        let mut tray_host = if let Some(registration) = context.try_resource::<TrayRegistration>() {
            let visibility_dispatcher = dispatcher.clone();
            let main_hwnd = hwnd.0 as isize;
            Some(
                Win32TrayHost::spawn(registration, context.clone(), hwnd, move |visible| {
                    let dispatcher = visibility_dispatcher.clone();
                    dispatcher.post(move || {
                        if visible {
                            show_window(HWND(main_hwnd as _));
                        } else {
                            hide_window(HWND(main_hwnd as _));
                        }
                    });
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
        #[cfg(feature = "tray")]
        if let Some(host) = tray_host.as_mut() {
            host.shutdown();
        }
        STATE.with(|state| state.borrow_mut().clear());
        Ok(())
    }
}

fn execute_window_command(
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
            } else if let Ok(hwnd) = create_window(
                HINSTANCE(instance as _),
                class_name,
                options,
                application_root_view(context.clone(), view),
                context,
                factory,
                dispatcher,
            ) {
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

fn create_window(
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
            options.size.width,
            options.size.height,
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
    set_window_corner_preference(
        hwnd,
        options.rounded_corners && initial_mode == WindowMode::Windowed,
    );
    let renderer_name = renderer_factory.name();
    let renderer = renderer_factory.create(hwnd).map_err(|source| {
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
    let mut session = UiSession::new();
    if let Some(executor) = context.task_spawner() {
        session.set_task_spawner(executor);
    }
    STATE.with(|state| {
        state.borrow_mut().insert(
            hwnd.0 as isize,
            WindowState {
                id: options.id,
                view,
                context,
                session,
                renderer: Some(renderer),
                renderer_factory,
                logical_size: options.size,
                minimum_size: options.minimum_size,
                maximum_size: options.maximum_size,
                resizable: options.resizable,
                native_titlebar: options.native_titlebar,
                rounded_corners: options.rounded_corners,
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
                render_retry_used: false,
                suppressed_ime_char_units: VecDeque::new(),
                pending_high_surrogate: None,
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

fn install_window_icons(hwnd: HWND) {
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

fn render_hidden_window_once(hwnd: HWND) {
    let target = unsafe { GetDC(Some(hwnd)) };
    if target.is_invalid() {
        return;
    }
    render_window(hwnd, target);
    unsafe {
        let _ = ReleaseDC(Some(hwnd), target);
    }
}

fn window_style(options: &WindowOptions) -> WINDOW_STYLE {
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

fn set_runtime_window_style(hwnd: HWND, style: WINDOW_STYLE) {
    let current = WINDOW_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32);
    let visibility = WINDOW_STYLE(current.0 & WS_VISIBLE.0);
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style | visibility).0 as isize);
    }
}

fn set_window_corner_preference(hwnd: HWND, rounded: bool) {
    let preference = window_corner_preference(rounded);
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

fn window_corner_preference(rounded: bool) -> DWM_WINDOW_CORNER_PREFERENCE {
    if rounded {
        DWMWCP_ROUND
    } else {
        DWMWCP_DONOTROUND
    }
}

enum WindowModeTransition {
    Fullscreen,
    Windowed {
        rounded_corners: bool,
        style: WINDOW_STYLE,
        placement: Option<WINDOWPLACEMENT>,
    },
}

fn set_window_mode(hwnd: HWND, mode: WindowMode) {
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
                rounded_corners: window.rounded_corners,
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
            set_window_corner_preference(hwnd, false);
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
            rounded_corners,
            style,
            placement,
        } => {
            set_window_corner_preference(hwnd, rounded_corners);
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

fn hwnd_for_id(id: &WindowId) -> Option<HWND> {
    STATE.with(|state| {
        state
            .borrow()
            .iter()
            .find_map(|(raw, state)| (state.id == *id).then_some(HWND(*raw as _)))
    })
}

fn set_desired_visibility(hwnd: HWND, visible: bool) {
    STATE.with(|state| {
        if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            window.visibility.set_desired(visible);
        }
    });
}

fn hide_window(hwnd: HWND) {
    set_desired_visibility(hwnd, false);
    hide_owned_windows(hwnd);
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    suspend_window_rendering(hwnd, false);
    suspend_application_if_backgrounded();
}

fn show_window(hwnd: HWND) {
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

fn suspend_window_rendering(hwnd: HWND, force: bool) -> bool {
    STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return false;
        };
        if window.rendering_suspended || (!force && !window.background_memory_optimization) {
            return false;
        }
        window.renderer.take();
        window.session.suspend_rendering();
        window.rendering_suspended = true;
        true
    })
}

fn resume_window_rendering(hwnd: HWND) {
    STATE.with(|state| {
        if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            window.rendering_suspended = false;
        }
    });
}

fn suspend_application_if_backgrounded() {
    if !application_is_backgrounded() {
        return;
    }

    let windows = STATE.with(|state| state.borrow().keys().copied().collect::<Vec<_>>());
    for raw in windows {
        suspend_window_rendering(HWND(raw as _), true);
    }
    super::background::release_visual_caches();
    super::background::trim_process_working_set();
    schedule_background_retrim();
}

fn application_is_backgrounded() -> bool {
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

fn schedule_background_retrim() {
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
                    super::background::trim_process_working_set();
                }
            });
        });
}

fn hide_owned_windows(owner: HWND) {
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

fn restore_owned_windows(owner: HWND) {
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

fn reposition_owned_windows(owner: HWND) {
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

fn position_window(hwnd: HWND) {
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
        (WindowPosition::Absolute { x, y }, _) => crate::core::Point::new(x, y),
        (WindowPosition::AdjacentToOwner { gap }, Some(owner)) => {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(owner, &mut rect) }.is_ok() {
                crate::core::Point::new(rect.right + gap, rect.top)
            } else {
                crate::core::Point::new(cursor.x + gap, cursor.y + gap)
            }
        }
        (WindowPosition::NearCursor { gap }, _) => {
            crate::core::Point::new(cursor.x + gap, cursor.y + gap)
        }
        (WindowPosition::AdjacentToOwner { gap }, None) => {
            crate::core::Point::new(cursor.x + gap, cursor.y + gap)
        }
        (WindowPosition::Centered, Some(owner)) => {
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(owner, &mut rect) }.is_ok() {
                crate::core::Point::new(
                    rect.left + ((rect.right - rect.left) - size.width) / 2,
                    rect.top + ((rect.bottom - rect.top) - size.height) / 2,
                )
            } else {
                crate::core::Point::new(cursor.x - size.width / 2, cursor.y - size.height / 2)
            }
        }
        (WindowPosition::Centered, None) => crate::core::Point::new(
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

fn install_wake(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    STATE.with(|state| {
        if let Some(state) = state.borrow().get(&raw) {
            state.session.set_wake(Arc::new(move || unsafe {
                let _ = PostMessageW(Some(HWND(raw as _)), WM_LGUI_DISPATCH, WPARAM(0), LPARAM(0));
            }));
        }
    });
}

fn custom_frame_hit_test(hwnd: HWND, lparam: LPARAM) -> Option<LRESULT> {
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
            .logical_point(Point::new(client_point.x, client_point.y));
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
                .logical_size(Size::new(client.right.max(1), client.bottom.max(1)));
            let excluded = state
                .drag_exclusion
                .map(|exclusion| exclusion(viewport.width, viewport.height))
                .is_some_and(|rect| rect.contains(logical_point));
            if logical_point.y >= 0 && logical_point.y < height && !excluded {
                return Some(LRESULT(HTCAPTION as isize));
            }
        }

        Some(LRESULT(HTCLIENT as isize))
    })
}

fn resize_border_hit(rect: RECT, point: Point, horizontal: i32, vertical: i32) -> u32 {
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

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCALCSIZE => {
            let custom_frame = STATE.with(|state| {
                state
                    .borrow()
                    .get(&(hwnd.0 as isize))
                    .is_some_and(|state| !state.native_titlebar)
            });
            if custom_frame {
                // The full window rectangle belongs to the client when native decorations are
                // disabled. Without handling this message Windows can retain a non-client caption
                // even after WS_CAPTION has been removed.
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_NCHITTEST => custom_frame_hit_test(hwnd, lparam)
            .unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }),
        WM_LGUI_DISPATCH => {
            drain_dispatcher(hwnd);
            LRESULT(0)
        }
        WM_LGUI_FRAME_TICK => {
            handle_frame_tick(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            let resize_dispatcher = STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let window = windows.get_mut(&(hwnd.0 as isize))?;
                if window.interaction_mode != WindowInteractionMode::Sizing {
                    return None;
                }
                window.resize_frame_throttle.request();
                Some(window.dispatcher.clone())
            });
            if wparam.0 as u32 == SIZE_MINIMIZED {
                hide_owned_windows(hwnd);
            } else if resize_dispatcher.is_none() {
                restore_owned_windows(hwnd);
            }
            if let Some(dispatcher) = resize_dispatcher {
                dispatcher.start_frame_driver();
            } else {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_MOVE => {
            reposition_owned_windows(hwnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ENTERSIZEMOVE => {
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                    window.interaction_mode = WindowInteractionMode::MoveResize;
                    window.resize_frame_throttle.reset();
                }
            });
            hide_owned_windows(hwnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_MOVING => {
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                    window.interaction_mode = WindowInteractionMode::Moving;
                }
            });
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_SIZING => {
            let dispatcher = STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let window = windows.get_mut(&(hwnd.0 as isize))?;
                if window.interaction_mode != WindowInteractionMode::Sizing {
                    window.interaction_mode = WindowInteractionMode::Sizing;
                    window.resize_frame_throttle.begin();
                }
                Some(window.dispatcher.clone())
            });
            if let Some(dispatcher) = dispatcher {
                dispatcher.start_frame_driver();
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            let exit = STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let window = windows.get_mut(&(hwnd.0 as isize))?;
                let was_sizing = window.interaction_mode == WindowInteractionMode::Sizing;
                window.interaction_mode = WindowInteractionMode::Idle;
                window.resize_frame_throttle.reset();
                Some((
                    was_sizing,
                    window
                        .can_advance_animations()
                        .then(|| window.dispatcher.clone()),
                ))
            });
            restore_owned_windows(hwnd);
            if let Some((was_sizing, dispatcher)) = exit {
                if was_sizing {
                    request_window_repaint(hwnd, WindowRepaint::Full);
                }
                if let Some(dispatcher) = dispatcher {
                    dispatcher.start_frame_driver();
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                let hide = STATE.with(|state| {
                    state
                        .borrow()
                        .get(&(hwnd.0 as isize))
                        .is_some_and(|state| state.hide_on_deactivate)
                });
                if hide {
                    hide_window(hwnd);
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_GETMINMAXINFO => {
            STATE.with(|state| {
                let state = state.borrow();
                let Some(state) = state.get(&(hwnd.0 as isize)) else {
                    return;
                };
                let scale = DpiContext::for_window(hwnd, state.logical_size).scale;
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                if let Some(minimum) = state.minimum_size {
                    let physical = scale.physical_size(minimum);
                    info.ptMinTrackSize.x = physical.width;
                    info.ptMinTrackSize.y = physical.height;
                }
                if let Some(maximum) = state.maximum_size {
                    let physical = scale.physical_size(maximum);
                    info.ptMaxTrackSize.x = physical.width;
                    info.ptMaxTrackSize.y = physical.height;
                }
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let suggested = unsafe { *(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            dispatch_input(
                hwnd,
                InputEvent::PointerDown {
                    point: logical_point(hwnd, unpack_point(lparam)),
                    button: PointerButton::Left,
                },
            );
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            dispatch_input(
                hwnd,
                InputEvent::PointerUp {
                    point: logical_point(hwnd, unpack_point(lparam)),
                    button: PointerButton::Left,
                },
            );
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            dispatch_input(
                hwnd,
                InputEvent::PointerMove(logical_point(hwnd, unpack_point(lparam))),
            );
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let mut point = POINT {
                x: lparam.0 as i16 as i32,
                y: (lparam.0 >> 16) as i16 as i32,
            };
            unsafe {
                let _ = ScreenToClient(hwnd, &mut point);
            }
            dispatch_input(
                hwnd,
                InputEvent::Wheel {
                    point: logical_point(hwnd, Point::new(point.x, point.y)),
                    delta_y: ((wparam.0 >> 16) as i16 as i32) / 120,
                },
            );
            LRESULT(0)
        }
        WM_CHAR => {
            let unit = wparam.0 as u16;
            if let Some(text) =
                take_window_char(hwnd, unit).filter(|text| !text.chars().all(char::is_control))
            {
                dispatch_input(hwnd, InputEvent::TextInput(text));
            }
            LRESULT(0)
        }
        WM_IME_STARTCOMPOSITION => {
            dispatch_input(hwnd, InputEvent::ImeStart);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_IME_COMPOSITION => {
            let flags = lparam.0 as u32;
            if flags & GCS_RESULTSTR.0 != 0 {
                if let Some(text) = read_ime_string(hwnd, GCS_RESULTSTR) {
                    STATE.with(|state| {
                        if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                            window.suppressed_ime_char_units.extend(text.encode_utf16());
                            window.pending_high_surrogate = None;
                        }
                    });
                    dispatch_input(hwnd, InputEvent::ImeCommit(text));
                }
            } else if flags & GCS_COMPSTR.0 != 0 {
                let text = read_ime_string(hwnd, GCS_COMPSTR).unwrap_or_default();
                dispatch_input(hwnd, InputEvent::ImeUpdate(text));
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_IME_ENDCOMPOSITION => {
            dispatch_input(hwnd, InputEvent::ImeEnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_KEYDOWN => {
            let key = key_code(wparam.0);
            match key {
                Some(KeyCode::Backspace) => dispatch_input(hwnd, InputEvent::Backspace),
                Some(key) => dispatch_input(
                    hwnd,
                    InputEvent::KeyDown {
                        key,
                        modifiers: key_modifiers(),
                    },
                ),
                None => {}
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            request_window_close(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let (empty, dispatcher, next_window) = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let dispatcher = state
                    .remove(&(hwnd.0 as isize))
                    .map(|window| window.dispatcher);
                let next_window = state.keys().next().copied();
                (state.is_empty(), dispatcher, next_window)
            });
            if let Some(dispatcher) = dispatcher {
                dispatcher.detach(hwnd);
                if let Some(raw) = next_window {
                    dispatcher.attach(HWND(raw as _));
                } else {
                    dispatcher.stop_frame_driver();
                }
            }
            if empty {
                unsafe {
                    PostQuitMessage(0);
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn request_window_close(hwnd: HWND) {
    let request = STATE.with(|state| {
        state.borrow().get(&(hwnd.0 as isize)).map(|state| {
            (
                state.close_policy,
                state.close_handler,
                state.context.clone(),
                state.id.clone(),
            )
        })
    });
    let Some((policy, handler, context, id)) = request else {
        return;
    };
    match policy {
        ClosePolicy::Exit => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        ClosePolicy::Hide => {
            hide_window(hwnd);
        }
        ClosePolicy::Notify => {
            if let Some(handler) = handler {
                let mut event = crate::core::UiEventContext::new(context, id);
                handler(&mut event);
                if event.flags().needs_frame {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
        }
    }
}

fn drain_dispatcher(hwnd: HWND) {
    let dispatcher = STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|window| window.dispatcher.clone())
    });
    let Some(dispatcher) = dispatcher else {
        return;
    };
    let result = dispatcher.drain();
    let mut start_frame_driver = result.frame_requested;
    let windows = STATE.with(|state| state.borrow().keys().copied().collect::<Vec<_>>());
    for raw in windows {
        let target = HWND(raw as _);
        let (repaint, frame_requested) = apply_pending_window_updates(target);
        start_frame_driver |= frame_requested;
        request_window_repaint(target, repaint);
    }
    if start_frame_driver {
        dispatcher.start_frame_driver();
    }
}

fn handle_frame_tick(hwnd: HWND) {
    let dispatcher = STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|window| window.dispatcher.clone())
    });
    let Some(dispatcher) = dispatcher else {
        return;
    };
    let elapsed_ms = dispatcher.frame_elapsed_ms();
    let (repaints, should_continue, frame_interval_ms) = STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let mut repaints = Vec::new();
        let mut should_continue = false;
        let mut frame_interval_ms = None::<u64>;
        for (raw, window) in windows.iter_mut() {
            if window.rendering_suspended
                || !window.visibility.desired_visible
                || window.visibility.hidden_for_owner
            {
                continue;
            }
            match window.interaction_mode {
                WindowInteractionMode::MoveResize | WindowInteractionMode::Moving => continue,
                WindowInteractionMode::Sizing => {
                    should_continue = true;
                    frame_interval_ms = Some(
                        frame_interval_ms.map_or(INTERACTIVE_RESIZE_FRAME_INTERVAL_MS, |current| {
                            current.min(INTERACTIVE_RESIZE_FRAME_INTERVAL_MS)
                        }),
                    );
                    if window.resize_frame_throttle.advance(elapsed_ms) {
                        repaints.push((HWND(*raw as _), WindowRepaint::Full));
                    }
                    continue;
                }
                WindowInteractionMode::Idle => {}
            }
            let output = window.session.advance(elapsed_ms);
            should_continue |= output.animation_changed;
            if output.animation_changed {
                let interval = window
                    .session
                    .runtime()
                    .frame_interval_ms()
                    .unwrap_or(DEFAULT_FRAME_INTERVAL_MS);
                frame_interval_ms =
                    Some(frame_interval_ms.map_or(interval, |current| current.min(interval)));
            }
            let repaint = if let Some(bounds) = output.dirty_bounds {
                window.session.invalidations_mut().invalidate_rect(bounds);
                WindowRepaint::Rect(bounds)
            } else if output.animation_changed {
                window.session.invalidate_all();
                WindowRepaint::Full
            } else {
                WindowRepaint::None
            };
            if repaint != WindowRepaint::None {
                repaints.push((HWND(*raw as _), repaint));
            }
        }
        (repaints, should_continue, frame_interval_ms)
    });
    for (target, repaint) in repaints {
        request_window_repaint(target, repaint);
    }
    dispatcher.finish_frame_tick_with_interval(
        should_continue,
        frame_interval_ms.unwrap_or(DEFAULT_FRAME_INTERVAL_MS),
    );
}

fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let target = unsafe { BeginPaint(hwnd, &mut paint) };
    render_window(hwnd, target);
    unsafe {
        let _ = EndPaint(hwnd, &paint);
    }
}

fn render_window(hwnd: HWND, target: HDC) {
    let retry = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.get_mut(&(hwnd.0 as isize)) else {
            return false;
        };
        let mut client = RECT::default();
        if let Err(source) = unsafe { GetClientRect(hwnd, &mut client) } {
            report_render_error(
                state,
                RenderErrorStage::Prepare,
                "read_client_bounds",
                &source,
            );
            return schedule_render_retry(state);
        }
        let physical = Size::new(client.right.max(1), client.bottom.max(1));
        let dpi = DpiContext::for_window(hwnd, state.logical_size);
        let logical = dpi.scale.logical_size(physical);
        let viewport = UiRect::new(0, 0, logical.width, logical.height);
        let commit = state.session.render_view(&state.view, viewport, dpi.scale);
        let physical_damage = commit
            .damage
            .dirty
            .effective_rects()
            .into_iter()
            .filter_map(|rect| {
                dpi.scale.physical_rect_outward(rect).intersect(UiRect::new(
                    0,
                    0,
                    physical.width,
                    physical.height,
                ))
            })
            .collect::<Vec<_>>();
        let scene = commit.scene.project_to_physical(dpi.scale);
        let physical_viewport = UiRect::new(0, 0, physical.width, physical.height);
        if state.renderer.is_none() {
            match state.renderer_factory.create(hwnd) {
                Ok(renderer) => state.renderer = Some(renderer),
                Err(source) => {
                    report_render_error(
                        state,
                        RenderErrorStage::Create,
                        "recreate_renderer",
                        &source,
                    );
                    return schedule_render_retry(state);
                }
            }
        }
        #[cfg(feature = "images")]
        let result = {
            let resources = state
                .context
                .try_resource::<crate::assets::RenderResources>()
                .map(|resources| (*resources).clone())
                .unwrap_or_default();
            let Some(renderer) = state.renderer.as_mut() else {
                return false;
            };
            crate::assets::with_render_resources(resources, || {
                renderer.draw_damage(hwnd, target, &scene, physical_viewport, &physical_damage)
            })
        };
        #[cfg(not(feature = "images"))]
        let result = state
            .renderer
            .as_mut()
            .expect("renderer was created before drawing")
            .draw_damage(hwnd, target, &scene, physical_viewport, &physical_damage);
        match result {
            Ok(()) => {
                state.render_retry_used = false;
                state.session.runtime().run_effects();
                false
            }
            Err(error) => {
                let source = error.source_error();
                state.context.report_render_error(RenderError::new(
                    state.id.clone(),
                    state.renderer_factory.name(),
                    error.stage(),
                    error.operation(),
                    source.code().0,
                    source.to_string(),
                ));
                schedule_render_retry(state)
            }
        }
    });
    if retry {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn report_render_error(
    state: &WindowState,
    stage: RenderErrorStage,
    operation: &'static str,
    source: &Error,
) {
    state.context.report_render_error(RenderError::new(
        state.id.clone(),
        state.renderer_factory.name(),
        stage,
        operation,
        source.code().0,
        source.to_string(),
    ));
}

fn schedule_render_retry(state: &mut WindowState) -> bool {
    state.session.invalidate_all();
    if state.render_retry_used {
        return false;
    }
    state.render_retry_used = true;
    true
}

fn dispatch_input(hwnd: HWND, input: InputEvent) {
    let (repaint, ime_update, frame_dispatcher) = STATE.with(|state| {
        if let Some(state) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            let dispatcher = state.dispatcher.clone();
            let output = state.session.handle_input(input);
            let session = &mut state.session;
            let mut context_requested_frame = false;
            let output = dispatch_runtime_output(
                output,
                &state.context,
                &state.id,
                |action| session.runtime_mut().handle_default_action(action),
                |context| context_requested_frame |= context.flags().needs_frame,
            );
            let ime_update =
                ime_composition_point(&output).map(|point| (state.logical_size, point));
            if context_requested_frame {
                session.invalidate_all();
                return (WindowRepaint::Full, ime_update, Some(dispatcher));
            }
            // Router and Store observables synchronously queue the component that owns their
            // outlet/content boundary. Consume that queue now so this input pass invalidates the
            // old boundary; Host diff adds the new boundary after the local component rerenders.
            let (mut repaint, frame_requested) = pending_session_repaint(session);
            if let Some(bounds) = output.dirty_bounds {
                session.invalidations_mut().invalidate_rect(bounds);
                repaint = repaint.union(WindowRepaint::Rect(bounds));
            }
            if output.animation_changed && repaint == WindowRepaint::None {
                session.invalidate_all();
                return (WindowRepaint::Full, ime_update, Some(dispatcher));
            }
            let frame_dispatcher =
                (frame_requested || output.animation_changed).then_some(dispatcher);
            return (repaint, ime_update, frame_dispatcher);
        }
        (WindowRepaint::None, None, None)
    });
    if let Some((logical_size, point)) = ime_update {
        update_ime_composition_window(hwnd, logical_size, point);
    }
    request_window_repaint(hwnd, repaint);
    if let Some(dispatcher) = frame_dispatcher {
        dispatcher.start_frame_driver();
    }
}

fn ime_composition_point(output: &RuntimeOutput) -> Option<Option<Point>> {
    output
        .events
        .iter()
        .filter_map(|event| match event {
            UiEvent::FocusChanged { current, .. } => Some(
                current
                    .as_ref()
                    .map(|hit| Point::new(hit.rect.left + 12, hit.rect.bottom + 4)),
            ),
            _ => None,
        })
        .last()
}

fn update_ime_composition_window(hwnd: HWND, logical_size: Size, point: Option<Point>) {
    unsafe {
        let _ = DestroyCaret();
    }
    let Some(point) = point else {
        return;
    };
    let physical_point = DpiContext::for_window(hwnd, logical_size)
        .scale
        .physical_point(point);
    unsafe {
        let context = ImmGetContext(hwnd);
        if context.is_invalid() {
            return;
        }
        let form = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT {
                x: physical_point.x,
                y: physical_point.y,
            },
            rcArea: Default::default(),
        };
        let _ = ImmSetCompositionWindow(context, &form);
        let _ = ImmReleaseContext(hwnd, context);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowRepaint {
    None,
    Rect(UiRect),
    Full,
}

impl WindowRepaint {
    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::Rect(left), Self::Rect(right)) => Self::Rect(left.union(right)),
            (Self::Rect(rect), Self::None) | (Self::None, Self::Rect(rect)) => Self::Rect(rect),
            (Self::None, Self::None) => Self::None,
        }
    }
}

fn pending_session_repaint(session: &mut UiSession) -> (WindowRepaint, bool) {
    let updates = session.apply_pending_updates();
    if updates.focus_changed {
        return (WindowRepaint::Full, updates.frame_requested);
    }
    if updates.dirty_ids.is_empty() {
        return (WindowRepaint::None, updates.frame_requested);
    }
    let repaint = session
        .tree()
        .paint_bounds(updates.dirty_ids)
        .map(WindowRepaint::Rect)
        .unwrap_or(WindowRepaint::Full);
    (repaint, updates.frame_requested)
}

fn apply_pending_window_updates(hwnd: HWND) -> (WindowRepaint, bool) {
    STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return (WindowRepaint::None, false);
        };
        pending_session_repaint(&mut window.session)
    })
}

fn request_window_repaint(hwnd: HWND, repaint: WindowRepaint) {
    if repaint != WindowRepaint::None {
        STATE.with(|state| {
            if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                window.render_retry_used = false;
            }
        });
    }
    match repaint {
        WindowRepaint::None => {}
        WindowRepaint::Full => unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        },
        WindowRepaint::Rect(logical) => {
            let physical = STATE.with(|state| {
                state.borrow().get(&(hwnd.0 as isize)).map(|window| {
                    DpiContext::for_window(hwnd, window.logical_size)
                        .scale
                        .physical_rect_outward(logical)
                })
            });
            if let Some(physical) = physical {
                let native = RECT {
                    left: physical.left,
                    top: physical.top,
                    right: physical.right,
                    bottom: physical.bottom,
                };
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), Some(&native), false);
                }
            }
        }
    }
}

fn logical_point(hwnd: HWND, point: Point) -> Point {
    STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|state| DpiContext::for_window(hwnd, state.logical_size).logical_point(point))
            .unwrap_or(point)
    })
}

fn unpack_point(lparam: LPARAM) -> Point {
    Point::new(lparam.0 as i16 as i32, (lparam.0 >> 16) as i16 as i32)
}

fn key_code(value: usize) -> Option<KeyCode> {
    match value {
        0x08 => Some(KeyCode::Backspace),
        0x09 => Some(KeyCode::Tab),
        0x0D => Some(KeyCode::Enter),
        0x25 => Some(KeyCode::ArrowLeft),
        0x26 => Some(KeyCode::ArrowUp),
        0x27 => Some(KeyCode::ArrowRight),
        0x28 => Some(KeyCode::ArrowDown),
        0x41 => Some(KeyCode::A),
        0x43 => Some(KeyCode::C),
        0x56 => Some(KeyCode::V),
        _ => None,
    }
}

fn key_modifiers() -> KeyModifiers {
    KeyModifiers {
        ctrl: unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0,
        shift: unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0,
    }
}

fn take_window_char(hwnd: HWND, unit: u16) -> Option<String> {
    STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return Some(String::from_utf16_lossy(&[unit]));
        };
        if suppress_committed_ime_char(&mut window.suppressed_ime_char_units, unit) {
            return None;
        }
        decode_utf16_char_unit(&mut window.pending_high_surrogate, unit)
    })
}

fn suppress_committed_ime_char(pending: &mut VecDeque<u16>, unit: u16) -> bool {
    if pending.front().copied() == Some(unit) {
        pending.pop_front();
        true
    } else {
        pending.clear();
        false
    }
}

fn decode_utf16_char_unit(pending_high_surrogate: &mut Option<u16>, unit: u16) -> Option<String> {
    if (0xD800..=0xDBFF).contains(&unit) {
        *pending_high_surrogate = Some(unit);
        return None;
    }
    let units = pending_high_surrogate
        .take()
        .map_or_else(|| vec![unit], |high| vec![high, unit]);
    Some(String::from_utf16_lossy(&units))
}

fn read_ime_string(
    hwnd: HWND,
    kind: windows::Win32::UI::Input::Ime::IME_COMPOSITION_STRING,
) -> Option<String> {
    unsafe {
        let context = ImmGetContext(hwnd);
        if context.is_invalid() {
            return None;
        }
        let byte_len = ImmGetCompositionStringW(context, kind, None, 0);
        if byte_len < 0 {
            let _ = ImmReleaseContext(hwnd, context);
            return None;
        }
        let mut units = vec![0_u16; byte_len as usize / size_of::<u16>()];
        if byte_len > 0 {
            let copied = ImmGetCompositionStringW(
                context,
                kind,
                Some(units.as_mut_ptr().cast()),
                byte_len as u32,
            );
            if copied < 0 {
                let _ = ImmReleaseContext(hwnd, context);
                return None;
            }
            units.truncate(copied as usize / size_of::<u16>());
        }
        let _ = ImmReleaseContext(hwnd, context);
        Some(String::from_utf16_lossy(&units))
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use windows::Win32::Foundation::RECT;

    use super::{
        can_advance_window_animations, decode_utf16_char_unit, resize_border_hit,
        suppress_committed_ime_char, window_corner_preference, window_style, OwnerVisibility,
        Point, ResizeFrameThrottle, WindowInteractionMode, WindowOptions, DWMWCP_DONOTROUND,
        DWMWCP_ROUND, HTBOTTOMRIGHT, HTCLIENT, HTTOPLEFT, WS_CAPTION, WS_POPUP, WS_THICKFRAME,
    };

    #[test]
    fn committed_ime_units_are_suppressed_until_the_sequence_diverges() {
        let mut pending = "\u{4E2D}\u{6587}".encode_utf16().collect::<VecDeque<_>>();
        assert!(suppress_committed_ime_char(&mut pending, '\u{4E2D}' as u16));
        assert!(!suppress_committed_ime_char(&mut pending, 'x' as u16));
        assert!(pending.is_empty());
    }

    #[test]
    fn utf16_decoder_combines_a_surrogate_pair() {
        let mut high = None;
        assert_eq!(decode_utf16_char_unit(&mut high, 0xD83D), None);
        assert_eq!(
            decode_utf16_char_unit(&mut high, 0xDE00),
            Some("\u{1F600}".to_string())
        );
        assert_eq!(high, None);
    }

    #[test]
    fn corner_preferences_map_to_explicit_dwm_requests() {
        assert_eq!(window_corner_preference(true), DWMWCP_ROUND);
        assert_eq!(window_corner_preference(false), DWMWCP_DONOTROUND);
    }

    #[test]
    fn custom_frames_remove_the_caption_and_keep_only_requested_resize_capability() {
        let decorated = window_style(&WindowOptions::new("decorated"));
        let custom = window_style(&WindowOptions::new("custom").native_titlebar(false));
        let fixed = window_style(
            &WindowOptions::new("fixed")
                .native_titlebar(false)
                .resizable(false),
        );

        assert_ne!(decorated.0 & WS_CAPTION.0, 0);
        assert_eq!(decorated.0 & WS_POPUP.0, 0);
        assert_eq!(custom.0 & WS_CAPTION.0, 0);
        assert_ne!(custom.0 & WS_POPUP.0, 0);
        assert_ne!(custom.0 & WS_THICKFRAME.0, 0);
        assert_eq!(fixed.0 & WS_CAPTION.0, 0);
        assert_ne!(fixed.0 & WS_POPUP.0, 0);
        assert_eq!(fixed.0 & WS_THICKFRAME.0, 0);
    }

    #[test]
    fn custom_frame_resize_hit_testing_includes_edges_and_corners() {
        let rect = RECT {
            left: 100,
            top: 200,
            right: 900,
            bottom: 700,
        };

        assert_eq!(
            resize_border_hit(rect, Point::new(101, 201), 8, 8),
            HTTOPLEFT
        );
        assert_eq!(
            resize_border_hit(rect, Point::new(899, 699), 8, 8),
            HTBOTTOMRIGHT
        );
        assert_eq!(
            resize_border_hit(rect, Point::new(500, 400), 8, 8),
            HTCLIENT
        );
    }

    #[test]
    fn owner_hide_and_restore_preserve_requested_visibility() {
        let mut visible_child = OwnerVisibility::visible();
        assert!(visible_child.hide_for_owner());
        assert!(visible_child.hidden_for_owner);
        assert!(visible_child.restore_for_owner());
        assert!(!visible_child.hidden_for_owner);
        assert!(visible_child.desired_visible);

        let mut explicitly_hidden_child = OwnerVisibility::visible();
        explicitly_hidden_child.set_desired(false);
        assert!(!explicitly_hidden_child.hide_for_owner());
        assert!(!explicitly_hidden_child.restore_for_owner());
        assert!(!explicitly_hidden_child.desired_visible);
        assert!(!explicitly_hidden_child.hidden_for_owner);
    }

    #[test]
    fn explicit_hide_while_owner_is_hidden_prevents_restore() {
        let mut child = OwnerVisibility::visible();
        assert!(child.hide_for_owner());
        child.set_desired(false);

        assert!(!child.restore_for_owner());
        assert!(!child.desired_visible);
        assert!(!child.hidden_for_owner);
    }

    #[test]
    fn only_visible_idle_windows_advance_animations() {
        let visible = OwnerVisibility::visible();
        assert!(can_advance_window_animations(
            false,
            visible,
            WindowInteractionMode::Idle
        ));
        assert!(!can_advance_window_animations(
            false,
            visible,
            WindowInteractionMode::MoveResize
        ));
        assert!(!can_advance_window_animations(
            false,
            visible,
            WindowInteractionMode::Moving
        ));
        assert!(!can_advance_window_animations(
            false,
            visible,
            WindowInteractionMode::Sizing
        ));
        assert!(!can_advance_window_animations(
            true,
            visible,
            WindowInteractionMode::Idle
        ));

        let mut hidden_for_owner = visible;
        assert!(hidden_for_owner.hide_for_owner());
        assert!(!can_advance_window_animations(
            false,
            hidden_for_owner,
            WindowInteractionMode::Idle
        ));
    }

    #[test]
    fn interactive_resize_emits_first_pending_frame_on_next_tick() {
        let mut throttle = ResizeFrameThrottle::default();
        throttle.begin();
        throttle.request();

        assert!(throttle.advance(1.0));
    }

    #[test]
    fn interactive_resize_coalesces_requests_until_frame_interval_elapses() {
        let mut throttle = ResizeFrameThrottle::default();
        throttle.begin();
        throttle.request();
        assert!(throttle.advance(1.0));

        throttle.request();
        assert!(!throttle.advance(10.0));
        throttle.request();
        assert!(!throttle.advance(22.0));
        assert!(throttle.advance(1.0));
    }

    #[test]
    fn interactive_resize_does_not_emit_without_a_pending_request() {
        let mut throttle = ResizeFrameThrottle::default();
        throttle.begin();

        assert!(!throttle.advance(33.0));
    }
}
