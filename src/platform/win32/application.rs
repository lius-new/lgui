use std::{
    cell::RefCell,
    collections::HashMap,
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
                    ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext, GCS_COMPSTR,
                    GCS_RESULTSTR,
                },
                KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT},
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetCursorPos, GetMessageW, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
                IsWindowVisible, IsZoomed, LoadCursorW, PostQuitMessage, RegisterClassExW,
                SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos,
                ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWL_STYLE,
                HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP,
                HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, LWA_ALPHA, MINMAXINFO, MSG, SIZE_MINIMIZED,
                SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CYSIZEFRAME, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SW_HIDE, SW_SHOW, WA_INACTIVE,
                WINDOWPLACEMENT, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE,
                WM_DESTROY, WM_DPICHANGED, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE,
                WM_GETMINMAXINFO, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
                WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_MOUSEWHEEL, WM_MOVE, WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_SIZE,
                WNDCLASSEXW, WS_CAPTION, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
                WS_VISIBLE,
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
        WindowCloseHandler, WindowCommand, WindowDragExclusion, WindowId, WindowMode,
        WindowOptions, WindowPosition,
    },
    core::{
        dispatch_runtime_output, InputEvent, KeyCode, KeyModifiers, Point, PointerButton, Size,
        UiRect,
    },
    session::UiSession,
};

#[cfg(feature = "renderer-gdi")]
use super::GdiRenderer;
#[cfg(feature = "tray")]
use super::Win32TrayHost;
use super::{dispatcher::WM_LGUI_DISPATCH, set_scale_preference, DpiContext, Win32Dispatcher};

const WINDOW_CLASS: &str = "LguiApplicationWindow";
const BACKGROUND_RETRIM_DELAY: Duration = Duration::from_secs(3);
static BACKGROUND_TRIM_GENERATION: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static STATE: RefCell<HashMap<isize, WindowState>> = RefCell::new(HashMap::new());
}

pub trait Win32Renderer: 'static {
    fn draw(
        &mut self,
        hwnd: HWND,
        target: HDC,
        scene: &crate::core::Scene,
        viewport: UiRect,
    ) -> bool;
}

pub trait Win32RendererFactory: Send + Sync + 'static {
    fn create(&self, hwnd: HWND) -> Result<Box<dyn Win32Renderer>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GdiRendererFactory;

#[cfg(feature = "renderer-gdi")]
impl Win32RendererFactory for GdiRendererFactory {
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
    ) -> bool {
        self.clear(target, viewport);
        #[cfg(feature = "advanced-rendering")]
        super::enhanced::GdiRenderer::draw_scene(target, scene);
        #[cfg(not(feature = "advanced-rendering"))]
        crate::renderer::RenderBackend::draw_scene(self, target, scene, None);
        true
    }
}

pub struct Win32Application {
    renderer_factory: Arc<dyn Win32RendererFactory>,
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
    visibility: OwnerVisibility,
    close_policy: ClosePolicy,
    close_handler: Option<WindowCloseHandler>,
    dispatcher: Win32Dispatcher,
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
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
        let class_name = wide(options.class_name.as_deref().unwrap_or(WINDOW_CLASS));
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: cursor,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassExW(&class) } == 0 {
            return Err(Error::from_thread());
        }

        let dispatcher = Win32Dispatcher::new();
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
        context.resources().provide(dispatcher.application_handle());
        #[cfg(feature = "notifications")]
        if let Some(registration) = context.try_resource::<NotificationRegistration>() {
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
            Arc::new(move || dispatcher.request_frame())
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
    if !options.native_titlebar {
        set_runtime_window_style(hwnd, style);
    }
    set_window_corner_preference(
        hwnd,
        options.rounded_corners && initial_mode == WindowMode::Windowed,
    );
    let renderer = renderer_factory.create(hwnd)?;
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
                visibility: OwnerVisibility {
                    desired_visible: initially_visible,
                    hidden_for_owner: false,
                },
                close_policy: options.close_policy,
                close_handler: options.close_handler,
                dispatcher,
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

fn set_window_mode(hwnd: HWND, mode: WindowMode) {
    STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return;
        };
        if window.mode == mode {
            return;
        }
        match mode {
            WindowMode::Fullscreen => {
                set_window_corner_preference(hwnd, false);
                let mut placement = WINDOWPLACEMENT {
                    length: size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                };
                if unsafe { GetWindowPlacement(hwnd, &mut placement) }.is_ok() {
                    window.windowed_placement = Some(placement);
                }
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
            WindowMode::Windowed => {
                set_window_corner_preference(hwnd, window.rounded_corners);
                unsafe {
                    set_runtime_window_style(hwnd, window.windowed_style);
                    if let Some(placement) = window.windowed_placement.take() {
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
        window.mode = mode;
    });
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
                let _ = InvalidateRect(Some(HWND(raw as _)), None, false);
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
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            if wparam.0 as u32 == SIZE_MINIMIZED {
                hide_owned_windows(hwnd);
            } else {
                restore_owned_windows(hwnd);
            }
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_MOVE => {
            reposition_owned_windows(hwnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ENTERSIZEMOVE => {
            hide_owned_windows(hwnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            restore_owned_windows(hwnd);
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
            if let Some(character) = char::from_u32(wparam.0 as u32) {
                dispatch_input(hwnd, InputEvent::TextInput(character.to_string()));
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
                    dispatch_input(hwnd, InputEvent::ImeCommit(text));
                }
            } else if flags & GCS_COMPSTR.0 != 0 {
                dispatch_input(
                    hwnd,
                    InputEvent::ImeUpdate(read_ime_string(hwnd, GCS_COMPSTR).unwrap_or_default()),
                );
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_IME_ENDCOMPOSITION => {
            dispatch_input(hwnd, InputEvent::ImeEnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_KEYDOWN => {
            if let Some(key) = key_code(wparam.0) {
                dispatch_input(
                    hwnd,
                    InputEvent::KeyDown {
                        key,
                        modifiers: key_modifiers(),
                    },
                );
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
    if result.tasks_executed || result.frame_requested {
        let windows = STATE.with(|state| state.borrow().keys().copied().collect::<Vec<_>>());
        for raw in windows {
            unsafe {
                let _ = InvalidateRect(Some(HWND(raw as _)), None, false);
            }
        }
    }
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
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.get_mut(&(hwnd.0 as isize)) else {
            return;
        };
        let mut client = RECT::default();
        if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
            return;
        }
        let physical = Size::new(client.right.max(1), client.bottom.max(1));
        let dpi = DpiContext::for_window(hwnd, state.logical_size);
        let logical = dpi.scale.logical_size(physical);
        let viewport = UiRect::new(0, 0, logical.width, logical.height);
        let commit = state.session.render_view(&state.view, viewport, dpi.scale);
        let scene = commit.scene.project_to_physical(dpi.scale);
        let physical_viewport = UiRect::new(0, 0, physical.width, physical.height);
        if state.renderer.is_none() {
            state.renderer = state.renderer_factory.create(hwnd).ok();
        }
        #[cfg(feature = "images")]
        let presented = {
            let resources = state
                .context
                .try_resource::<crate::assets::RenderResources>()
                .map(|resources| (*resources).clone())
                .unwrap_or_default();
            let Some(renderer) = state.renderer.as_mut() else {
                return;
            };
            crate::assets::with_render_resources(resources, || {
                renderer.draw(hwnd, target, &scene, physical_viewport)
            })
        };
        #[cfg(not(feature = "images"))]
        let presented = state
            .renderer
            .as_mut()
            .is_some_and(|renderer| renderer.draw(hwnd, target, &scene, physical_viewport));
        if presented {
            state.session.runtime().run_effects();
        }
    });
}

fn dispatch_input(hwnd: HWND, input: InputEvent) {
    STATE.with(|state| {
        if let Some(state) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            let output = state.session.handle_input(input);
            let session = &mut state.session;
            let _ = dispatch_runtime_output(
                output,
                &state.context,
                &state.id,
                |action| session.runtime_mut().handle_default_action(action),
                |_| {},
            );
        }
    });
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
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
    use windows::Win32::Foundation::RECT;

    use super::{
        resize_border_hit, window_corner_preference, window_style, OwnerVisibility, Point,
        WindowOptions, DWMWCP_DONOTROUND, DWMWCP_ROUND, HTBOTTOMRIGHT, HTCLIENT, HTTOPLEFT,
        WS_CAPTION, WS_POPUP, WS_THICKFRAME,
    };

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
}
