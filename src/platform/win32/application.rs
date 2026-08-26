use std::{cell::RefCell, collections::HashMap, mem::size_of, sync::Arc};

use windows::{
    core::{Error, Result, HRESULT, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromWindow,
            ScreenToClient, UpdateWindow, HDC, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            Input::{
                Ime::{
                    ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext, GCS_COMPSTR,
                    GCS_RESULTSTR,
                },
                KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT},
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
                DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW,
                GetWindowPlacement, GetWindowRect, IsWindowVisible, LoadCursorW, LoadIconW,
                PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetLayeredWindowAttributes,
                SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow, TrackPopupMenu,
                TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWL_STYLE, IDC_ARROW,
                IDI_APPLICATION, LWA_ALPHA, MF_CHECKED, MF_GRAYED, MF_STRING, MINMAXINFO, MSG,
                SIZE_MINIMIZED, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
                SW_SHOW, TPM_RIGHTBUTTON, WA_INACTIVE, WINDOWPLACEMENT, WINDOW_EX_STYLE,
                WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_DESTROY,
                WM_DPICHANGED, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
                WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
                WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
                WM_MOVE, WM_PAINT, WM_RBUTTONUP, WM_SIZE, WNDCLASSEXW, WS_CAPTION, WS_EX_LAYERED,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_OVERLAPPED,
                WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SYSMENU,
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
        AppView, ApplicationBackend, ApplicationContext, ClosePolicy, WindowCloseHandler,
        WindowCommand, WindowId, WindowMode, WindowOptions, WindowPosition,
    },
    core::{
        dispatch_runtime_output, InputEvent, KeyCode, KeyModifiers, Point, PointerButton, Size,
        UiRect,
    },
    session::UiSession,
};

#[cfg(feature = "renderer-gdi")]
use super::GdiRenderer;
use super::{dispatcher::WM_LGUI_DISPATCH, set_scale_preference, DpiContext, Win32Dispatcher};
#[cfg(feature = "tray")]
use super::{taskbar_created_message, Win32TrayIcon, TRAY_MESSAGE_ID};

const WINDOW_CLASS: &str = "LguiApplicationWindow";
#[cfg(feature = "tray")]
const TRAY_COMMAND_BASE: usize = 0x4000;

thread_local! {
    static STATE: RefCell<HashMap<isize, WindowState>> = RefCell::new(HashMap::new());
    #[cfg(feature = "tray")]
    static TRAY: RefCell<Option<ApplicationTray>> = const { RefCell::new(None) };
}

#[cfg(feature = "tray")]
struct ApplicationTray {
    icon: Win32TrayIcon,
    registration: Arc<TrayRegistration>,
    context: ApplicationContext,
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
    renderer: Box<dyn Win32Renderer>,
    logical_size: Size,
    minimum_size: Option<Size>,
    windowed_style: WINDOW_STYLE,
    windowed_placement: Option<WINDOWPLACEMENT>,
    mode: WindowMode,
    owner: Option<HWND>,
    position: WindowPosition,
    hide_on_deactivate: bool,
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
        #[cfg(feature = "tray")]
        if let Some(registration) = context.try_resource::<TrayRegistration>() {
            install_tray(hwnd, registration, context.clone())?;
        }
        #[cfg(feature = "store")]
        context.stores().set_wake({
            let dispatcher = dispatcher.clone();
            Arc::new(move || dispatcher.request_frame())
        });
        let manager = context.windows();
        let instance_value = instance.0 as isize;
        let command_dispatcher = dispatcher.clone();
        manager.install(move |command| {
            let dispatcher = command_dispatcher.clone();
            let task_dispatcher = dispatcher.clone();
            let class_name = class_name.clone();
            let context = context.clone();
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
        debug_assert_eq!(hwnd_for_id(&main_id), Some(hwnd));

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        STATE.with(|state| state.borrow_mut().clear());
        #[cfg(feature = "tray")]
        TRAY.with(|tray| *tray.borrow_mut() = None);
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
                set_desired_visibility(hwnd, true);
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                }
                restore_owned_windows(hwnd);
            } else if let Ok(hwnd) = create_window(
                HINSTANCE(instance as _),
                class_name,
                options,
                view,
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
                unsafe {
                    let command = if IsWindowVisible(hwnd).as_bool() {
                        set_desired_visibility(hwnd, false);
                        hide_owned_windows(hwnd);
                        windows::Win32::UI::WindowsAndMessaging::SW_HIDE
                    } else {
                        set_desired_visibility(hwnd, true);
                        restore_owned_windows(hwnd);
                        SW_SHOW
                    };
                    let _ = ShowWindow(hwnd, command);
                }
            } else {
                let _ = create_window(
                    HINSTANCE(instance as _),
                    class_name,
                    options,
                    view,
                    context,
                    factory,
                    dispatcher,
                );
            }
        }
        WindowCommand::Hide(id) => {
            if let Some(hwnd) = hwnd_for_id(&id) {
                set_desired_visibility(hwnd, false);
                hide_owned_windows(hwnd);
                unsafe {
                    let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
                }
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
    let title = wide(&options.title);
    let style = if options.resizable {
        WS_OVERLAPPEDWINDOW
    } else {
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
    };
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
                renderer,
                logical_size: options.size,
                minimum_size: options.minimum_size,
                windowed_style: style,
                windowed_placement: None,
                mode: WindowMode::Windowed,
                owner,
                position: options.position,
                hide_on_deactivate: options.hide_on_deactivate,
                visibility: OwnerVisibility::visible(),
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
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
    }
    Ok(hwnd)
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
                        SetWindowLongPtrW(hwnd, GWL_STYLE, WS_POPUP.0 as isize);
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
            WindowMode::Windowed => unsafe {
                SetWindowLongPtrW(hwnd, GWL_STYLE, window.windowed_style.0 as isize);
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
            },
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
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
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
    let Some((owner, position, logical_size)) = STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|state| (state.owner, state.position, state.logical_size))
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
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER,
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

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    #[cfg(feature = "tray")]
    if message == TRAY_MESSAGE_ID {
        handle_tray_message(hwnd, lparam.0 as u32);
        return LRESULT(0);
    }
    #[cfg(feature = "tray")]
    if message == WM_COMMAND {
        let command_id = wparam.0 & 0xFFFF;
        if command_id >= TRAY_COMMAND_BASE {
            dispatch_tray_command(command_id - TRAY_COMMAND_BASE);
            return LRESULT(0);
        }
    }
    #[cfg(feature = "tray")]
    if message == taskbar_created_message() {
        TRAY.with(|tray| {
            if let Some(tray) = tray.borrow_mut().as_mut() {
                let _ = tray.icon.restore();
            }
        });
        return LRESULT(0);
    }
    match message {
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
                    set_desired_visibility(hwnd, false);
                    unsafe {
                        let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_GETMINMAXINFO => {
            STATE.with(|state| {
                let state = state.borrow();
                let Some((state, minimum)) = state
                    .get(&(hwnd.0 as isize))
                    .and_then(|state| state.minimum_size.map(|minimum| (state, minimum)))
                else {
                    return;
                };
                let physical = DpiContext::for_window(hwnd, state.logical_size)
                    .scale
                    .physical_size(minimum);
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                info.ptMinTrackSize.x = physical.width;
                info.ptMinTrackSize.y = physical.height;
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
                #[cfg(feature = "tray")]
                TRAY.with(|tray| *tray.borrow_mut() = None);
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
            set_desired_visibility(hwnd, false);
            hide_owned_windows(hwnd);
            unsafe {
                let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
            }
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
        #[cfg(feature = "images")]
        let presented = {
            let resources = state
                .context
                .try_resource::<crate::assets::RenderResources>()
                .map(|resources| (*resources).clone())
                .unwrap_or_default();
            crate::assets::with_render_resources(resources, || {
                state.renderer.draw(hwnd, target, &scene, physical_viewport)
            })
        };
        #[cfg(not(feature = "images"))]
        let presented = state.renderer.draw(hwnd, target, &scene, physical_viewport);
        if presented {
            state.session.runtime().run_effects();
        }
    });
    unsafe {
        let _ = EndPaint(hwnd, &paint);
    }
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

#[cfg(feature = "tray")]
fn install_tray(
    hwnd: HWND,
    registration: Arc<TrayRegistration>,
    context: ApplicationContext,
) -> Result<()> {
    let icon = unsafe { LoadIconW(None, IDI_APPLICATION) }?;
    let mut tray = Win32TrayIcon::new(hwnd, icon, registration.options.tooltip.clone());
    tray.install().map_err(|_| Error::from_thread())?;
    TRAY.with(|slot| {
        *slot.borrow_mut() = Some(ApplicationTray {
            icon: tray,
            registration,
            context,
        });
    });
    Ok(())
}

#[cfg(feature = "tray")]
fn handle_tray_message(hwnd: HWND, event: u32) {
    match event {
        WM_LBUTTONDBLCLK => {
            let command = TRAY.with(|tray| {
                tray.borrow()
                    .as_ref()
                    .and_then(|tray| tray.registration.options.activate_command.clone())
            });
            if let Some(command) = command {
                dispatch_tray_command_value(command);
            }
        }
        WM_RBUTTONUP => show_tray_menu(hwnd),
        _ => {}
    }
}

#[cfg(feature = "tray")]
fn show_tray_menu(hwnd: HWND) {
    let items = TRAY.with(|tray| {
        tray.borrow()
            .as_ref()
            .map(|tray| tray.registration.options.items.clone())
            .unwrap_or_default()
    });
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        let mut flags = MF_STRING;
        if !item.enabled {
            flags |= MF_GRAYED;
        }
        if item.checked {
            flags |= MF_CHECKED;
        }
        let label = wide(&item.label);
        let _ = unsafe {
            AppendMenuW(
                menu,
                flags,
                TRAY_COMMAND_BASE + index,
                PCWSTR(label.as_ptr()),
            )
        };
    }
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, None, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

#[cfg(feature = "tray")]
fn dispatch_tray_command(index: usize) {
    let command = TRAY.with(|tray| {
        tray.borrow()
            .as_ref()
            .and_then(|tray| tray.registration.options.items.get(index))
            .filter(|item| item.enabled)
            .map(|item| item.command.clone())
    });
    if let Some(command) = command {
        dispatch_tray_command_value(command);
    }
}

#[cfg(feature = "tray")]
fn dispatch_tray_command_value(command: String) {
    let invocation = TRAY.with(|tray| {
        tray.borrow()
            .as_ref()
            .map(|tray| (Arc::clone(&tray.registration.handler), tray.context.clone()))
    });
    if let Some((handler, context)) = invocation {
        handler(&context, &command);
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::OwnerVisibility;

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
