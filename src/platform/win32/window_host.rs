use windows::{
    core::Error,
    Win32::{
        Foundation::{
            COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT as WinPoint, RECT, SIZE, WPARAM,
        },
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            DWM_WINDOW_CORNER_PREFERENCE,
        },
        Graphics::Gdi::{
            GetDC, ReleaseDC, ScreenToClient, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, HDC,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowRect, IsWindowVisible,
                LoadCursorW, RegisterClassExW, SetCursor, SetForegroundWindow, SetWindowPos,
                ShowWindow, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HTCAPTION,
                IDC_ARROW, SWP_NOACTIVATE, SW_HIDE, SW_SHOW, ULW_ALPHA, WHEEL_DELTA, WM_CLOSE,
                WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN,
                WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_NCHITTEST, WM_PAINT,
                WM_SETCURSOR, WM_SETTINGCHANGE, WNDCLASSEXW, WNDPROC, WS_EX_LAYERED,
                WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::{
    core::{
        HostTree, InputEvent, Point, PointerButton, RuntimeOutput, Scene, Size, UiId, UiRect,
        UiRuntime, UiScale,
    },
    session::UiSession,
};

use super::{rect_size, DpiContext, LayeredBackbuffer};

const WM_MOUSELEAVE_MESSAGE: u32 = 0x02A3;

pub type LayeredWindowTreeBuilder = fn(HostTree, &UiRuntime, UiRect, UiScale) -> HostTree;
pub type LayeredWindowEventDispatcher = fn(&RuntimeOutput) -> LayeredWindowEventDispatch;
pub type DragExclusionRect = fn(i32, i32) -> UiRect;
pub type LayeredWindowSessionConfigurator = fn(&mut UiSession, HWND);
pub type LayeredWindowSceneRenderer = fn(HDC, &Scene, Option<UiRect>);
pub type LayeredWindowErrorReporter = fn(&'static str, &Error);

#[derive(Default)]
pub struct LayeredWindowEventDispatch {
    pub should_close: bool,
    pub default_prevented: std::collections::HashSet<UiId>,
}

#[derive(Clone, Copy)]
pub struct LayeredWindowConfig {
    pub class_name: &'static str,
    pub width: i32,
    pub height: i32,
    pub min_height: i32,
    pub radius: i32,
    pub position: LayeredWindowPosition,
    pub snapshot_on_background: bool,
    pub titlebar_drag_height: Option<i32>,
    pub drag_exclusion_rect: Option<DragExclusionRect>,
    pub scale_reference_size: Size,
    pub configure_session: LayeredWindowSessionConfigurator,
    pub draw_scene: LayeredWindowSceneRenderer,
    pub report_error: LayeredWindowErrorReporter,
}

#[derive(Clone, Copy)]
pub enum LayeredWindowPosition {
    AdjacentToOwner { gap: i32 },
    CenterScreen,
}

pub struct LayeredWindowHost {
    config: LayeredWindowConfig,
    hwnd: HWND,
    backbuffer: Option<LayeredBackbuffer>,
    background_snapshot: Option<LayeredWindowSnapshot>,
    session: UiSession,
    mouse_tracking: bool,
    window_height: i32,
    dpi_context: Option<DpiContext>,
    restore_after_owner_move: bool,
    restore_after_owner_background: bool,
}

struct LayeredWindowSnapshot {
    width: i32,
    height: i32,
    bgra: Vec<u8>,
}

impl LayeredWindowHost {
    pub fn new(config: LayeredWindowConfig) -> Self {
        Self {
            config,
            hwnd: HWND::default(),
            backbuffer: None,
            background_snapshot: None,
            session: UiSession::new(),
            mouse_tracking: false,
            window_height: 0,
            dpi_context: None,
            restore_after_owner_move: false,
            restore_after_owner_background: false,
        }
    }

    fn preferred_size(&self) -> Size {
        Size::new(self.config.width, self.config.height)
    }

    fn scale_reference_size(&self) -> Size {
        self.config.scale_reference_size
    }

    pub fn show(&mut self, owner: HWND, wnd_proc: WNDPROC, build_tree: LayeredWindowTreeBuilder) {
        self.restore_after_owner_background = false;
        if self.hwnd.is_invalid() && self.create(owner, wnd_proc).is_err() {
            return;
        }
        self.position(owner);
        self.present(build_tree);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
        self.present(build_tree);
    }

    pub fn restore_visible(
        &mut self,
        owner: HWND,
        wnd_proc: WNDPROC,
        build_tree: LayeredWindowTreeBuilder,
    ) {
        if self.hwnd.is_invalid() && self.create(owner, wnd_proc).is_err() {
            return;
        }
        self.position(owner);
        let snapshot_shown = self.show_background_snapshot();
        if !snapshot_shown {
            self.present(build_tree);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
        }
        self.present(build_tree);
        self.background_snapshot = None;
    }

    pub fn present(&mut self, build_tree: LayeredWindowTreeBuilder) {
        if self.hwnd.is_invalid() {
            return;
        }
        let Some(window) = read_window_state(self.hwnd) else {
            return;
        };
        let context = self
            .dpi_context
            .unwrap_or_else(|| DpiContext::for_window(self.hwnd, self.scale_reference_size()));
        self.dpi_context = Some(context);
        let logical_size = context
            .scale
            .logical_size(Size::new(window.width, window.height));
        self.window_height = logical_size.height;
        let screen_dc = unsafe { GetDC(None) };
        if screen_dc.is_invalid() {
            return;
        }
        if self.backbuffer.is_none()
            || self.backbuffer.as_ref().is_some_and(|buffer| {
                buffer.width() != window.width || buffer.height() != window.height
            })
        {
            self.backbuffer = LayeredBackbuffer::new(screen_dc, window.width, window.height);
        }
        let Some(backbuffer) = self.backbuffer.as_mut() else {
            unsafe {
                let _ = ReleaseDC(None, screen_dc);
            }
            return;
        };

        let physical_viewport = UiRect::new(0, 0, window.width, window.height);
        let logical_viewport = UiRect::new(0, 0, logical_size.width, logical_size.height);
        self.session.apply_pending_updates();
        self.session.prepare_render(logical_viewport, context.scale);
        let retained_tree = self.session.render_tree();
        let tree = build_tree(
            retained_tree,
            self.session.runtime(),
            logical_viewport,
            context.scale,
        );
        self.session.replace_tree(tree);
        let commit = self.session.commit(logical_viewport);
        let scene = commit.scene.project_to_physical(context.scale);
        backbuffer.clear_region(physical_viewport);
        (self.config.draw_scene)(backbuffer.hdc(), &scene, None);
        if self.config.radius > 0 {
            backbuffer.set_rounded_rect_alpha_mask(
                physical_viewport,
                context.scale.physical_length(self.config.radius),
            );
        } else {
            backbuffer.set_opaque_alpha();
        }
        match update_layered_window(self.hwnd, screen_dc, backbuffer, window) {
            Ok(()) => {
                backbuffer.mark_valid();
                self.session.runtime().run_effects();
            }
            Err(error) => {
                backbuffer.invalidate();
                (self.config.report_error)("failed to present layered auxiliary window", &error);
            }
        }

        unsafe {
            let _ = ReleaseDC(None, screen_dc);
        }
    }

    pub fn hide(&mut self) {
        if self.hwnd.is_invalid() {
            return;
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.session.runtime_mut().clear_interaction_state();
        self.mouse_tracking = false;
    }

    pub fn dismiss(&mut self) {
        self.restore_after_owner_move = false;
        self.restore_after_owner_background = false;
        self.background_snapshot = None;
        self.hide();
    }

    pub fn hide_for_owner_move(&mut self) {
        if !self.is_visible() {
            self.restore_after_owner_move = false;
            return;
        }
        self.restore_after_owner_move = true;
        self.hide();
    }

    pub fn restore_after_owner_move(
        &mut self,
        owner: HWND,
        wnd_proc: WNDPROC,
        build_tree: LayeredWindowTreeBuilder,
    ) {
        if !self.restore_after_owner_move {
            return;
        }
        self.restore_after_owner_move = false;
        self.restore_visible(owner, wnd_proc, build_tree);
    }

    pub fn hide_for_owner_background(&mut self) {
        let visible = self.is_visible();
        self.restore_after_owner_background = visible;
        if self.config.snapshot_on_background {
            self.background_snapshot = visible.then(|| self.capture_snapshot()).flatten();
        } else {
            self.background_snapshot = None;
        }
        self.restore_after_owner_move = false;
        self.hide();
    }

    pub fn restore_after_owner_background(
        &mut self,
        owner: HWND,
        wnd_proc: WNDPROC,
        build_tree: LayeredWindowTreeBuilder,
    ) {
        if !self.restore_after_owner_background {
            return;
        }
        self.restore_after_owner_background = false;
        self.restore_visible(owner, wnd_proc, build_tree);
    }

    pub fn is_visible(&self) -> bool {
        !self.hwnd.is_invalid() && unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn window_height(&self) -> i32 {
        self.window_height
    }

    pub fn handle_message(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        build_tree: LayeredWindowTreeBuilder,
        dispatch_events: LayeredWindowEventDispatcher,
    ) -> LRESULT {
        match message {
            WM_PAINT => {
                self.present(build_tree);
                LRESULT(0)
            }
            WM_SETCURSOR => {
                set_arrow_cursor();
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_NCHITTEST => {
                if self.is_titlebar_drag_area(hwnd, lparam) {
                    return LRESULT(HTCAPTION as isize);
                }
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
            WM_MOUSEMOVE => {
                set_arrow_cursor();
                self.track_mouse_leave(hwnd);
                self.handle_input(
                    InputEvent::PointerMove(unpack_point(lparam)),
                    build_tree,
                    dispatch_events,
                );
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                self.handle_input(
                    InputEvent::PointerDown {
                        point: unpack_point(lparam),
                        button: PointerButton::Left,
                    },
                    build_tree,
                    dispatch_events,
                );
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                self.handle_input(
                    InputEvent::PointerUp {
                        point: unpack_point(lparam),
                        button: PointerButton::Left,
                    },
                    build_tree,
                    dispatch_events,
                );
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                self.handle_input(
                    InputEvent::Wheel {
                        point: screen_point_to_client(hwnd, lparam),
                        delta_y: wheel_scroll_units(wheel_delta(wparam)),
                    },
                    build_tree,
                    dispatch_events,
                );
                LRESULT(0)
            }
            WM_MOUSELEAVE_MESSAGE => {
                self.mouse_tracking = false;
                self.handle_input(InputEvent::PointerLeave, build_tree, dispatch_events);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                self.apply_dpi_change(hwnd, wparam, lparam);
                self.present(build_tree);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
                self.refresh_current_monitor(hwnd);
                self.present(build_tree);
                LRESULT(0)
            }
            WM_MOVE => {
                if self.refresh_current_monitor_if_changed(hwnd) {
                    self.present(build_tree);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                self.hide();
                LRESULT(0)
            }
            WM_DESTROY => {
                self.hwnd = HWND::default();
                self.backbuffer = None;
                self.background_snapshot = None;
                self.session.clear_host();
                self.dpi_context = None;
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn create(&mut self, owner: HWND, wnd_proc: WNDPROC) -> Result<(), Error> {
        register_layered_window_class(self.config.class_name, wnd_proc)?;
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
        let class_name = widestring(self.config.class_name);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW,
                windows::core::PCWSTR(class_name.as_ptr()),
                windows::core::PCWSTR(class_name.as_ptr()),
                WS_POPUP,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                self.config.width,
                self.config.height,
                Some(owner),
                None,
                Some(instance),
                None,
            )
        }?;
        self.hwnd = hwnd;
        (self.config.configure_session)(&mut self.session, hwnd);
        apply_layered_window_chrome(hwnd);
        Ok(())
    }

    pub fn refresh_for_owner(&mut self, owner: HWND, build_tree: LayeredWindowTreeBuilder) {
        if !self.is_visible() {
            return;
        }
        self.position(owner);
        self.invalidate_scale_state();
        self.present(build_tree);
    }

    fn apply_dpi_change(&mut self, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
        let dpi = (wparam.0 & 0xFFFF) as u32;
        let suggested = unsafe { *(lparam.0 as *const RECT) };
        let center = Point::new(
            suggested.left + (suggested.right - suggested.left) / 2,
            suggested.top + (suggested.bottom - suggested.top) / 2,
        );
        let context = DpiContext::for_point(center, dpi, self.scale_reference_size());
        let size = context.scale.physical_size(self.preferred_size());
        let origin = context.clamp_origin(Point::new(suggested.left, suggested.top), size);
        self.dpi_context = Some(context);
        self.window_height = self.config.height;
        self.invalidate_scale_state();
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                origin.x,
                origin.y,
                size.width,
                size.height,
                SWP_NOACTIVATE,
            );
        }
    }

    fn refresh_current_monitor(&mut self, hwnd: HWND) {
        let context = DpiContext::for_window(hwnd, self.scale_reference_size());
        self.apply_current_monitor_context(hwnd, context);
    }

    fn refresh_current_monitor_if_changed(&mut self, hwnd: HWND) -> bool {
        let context = DpiContext::for_window(hwnd, self.scale_reference_size());
        if self
            .dpi_context
            .is_some_and(|current| current.same_monitor_metrics(context))
        {
            return false;
        }
        self.apply_current_monitor_context(hwnd, context);
        true
    }

    fn apply_current_monitor_context(&mut self, hwnd: HWND, context: DpiContext) {
        let size = context.scale.physical_size(self.preferred_size());
        let current = read_window_state(hwnd)
            .map(|window| Point::new(window.window_rect.left, window.window_rect.top))
            .unwrap_or(Point::new(
                context.work_area.rect.left,
                context.work_area.rect.top,
            ));
        let origin = context.clamp_origin(current, size);
        self.dpi_context = Some(context);
        self.window_height = self.config.height;
        self.invalidate_scale_state();
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                origin.x,
                origin.y,
                size.width,
                size.height,
                SWP_NOACTIVATE,
            );
        }
    }

    fn invalidate_scale_state(&mut self) {
        self.backbuffer = None;
        self.background_snapshot = None;
        self.session.clear_host();
        self.session.invalidate_all();
        self.session.runtime_mut().clear_interaction_state();
    }

    fn position(&mut self, owner: HWND) {
        let context = DpiContext::for_window(owner, self.scale_reference_size());
        self.dpi_context = Some(context);
        let work = context.work_area.rect;
        let width = context
            .scale
            .physical_length(self.config.width)
            .min(work.width())
            .max(1);
        let preferred_height = context.scale.physical_length(self.config.height);
        let min_height = context.scale.physical_length(self.config.min_height);
        let height = preferred_height.clamp(min_height.min(work.height()), work.height());
        let (x, y) = match self.config.position {
            LayeredWindowPosition::AdjacentToOwner { gap } => {
                let mut owner_rect = RECT::default();
                unsafe {
                    let _ = GetWindowRect(owner, &mut owner_rect);
                }
                let owner_height = owner_rect.bottom - owner_rect.top;
                let height = owner_height.clamp(min_height.min(work.height()), work.height());
                let gap = context.scale.physical_value(gap);
                let right_x = owner_rect.right + gap;
                let left_x = owner_rect.left - width - gap;
                let x = if right_x + width <= work.right {
                    right_x
                } else {
                    left_x
                }
                .clamp(work.left, work.right - width);
                let y = owner_rect.top.clamp(work.top, work.bottom - height);
                self.window_height = context.scale.logical_value(height).max(1);
                unsafe {
                    let _ = SetWindowPos(self.hwnd, None, x, y, width, height, SWP_NOACTIVATE);
                }
                return;
            }
            LayeredWindowPosition::CenterScreen => (
                work.left + (work.width() - width) / 2,
                work.top + (work.height() - height) / 2,
            ),
        };
        self.window_height = context.scale.logical_value(height).max(1);
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, width, height, SWP_NOACTIVATE);
        }
    }

    fn handle_input(
        &mut self,
        input: InputEvent,
        build_tree: LayeredWindowTreeBuilder,
        dispatch_events: LayeredWindowEventDispatcher,
    ) {
        if !self.session.has_tree() {
            return;
        }
        let input = self.logical_input(input);
        let output = self.session.handle_input(input);
        let feedback = dispatch_events(&output);
        if feedback.should_close {
            self.hide();
            return;
        }
        for action in output.default_actions {
            if feedback.default_prevented.contains(&action.event_target) {
                continue;
            }
            let default_output = self.session.runtime_mut().handle_default_action(action);
            if dispatch_events(&default_output).should_close {
                self.hide();
                return;
            }
        }
        self.present(build_tree);
    }

    fn logical_input(&self, input: InputEvent) -> InputEvent {
        let scale = self
            .dpi_context
            .map(|context| context.scale)
            .unwrap_or(UiScale::ONE);
        match input {
            InputEvent::PointerMove(point) => InputEvent::PointerMove(scale.logical_point(point)),
            InputEvent::PointerDown { point, button } => InputEvent::PointerDown {
                point: scale.logical_point(point),
                button,
            },
            InputEvent::PointerUp { point, button } => InputEvent::PointerUp {
                point: scale.logical_point(point),
                button,
            },
            InputEvent::Wheel { point, delta_y } => InputEvent::Wheel {
                point: scale.logical_point(point),
                delta_y,
            },
            other => other,
        }
    }

    fn capture_snapshot(&self) -> Option<LayeredWindowSnapshot> {
        let backbuffer = self.backbuffer.as_ref()?;
        Some(LayeredWindowSnapshot {
            width: backbuffer.width(),
            height: backbuffer.height(),
            bgra: backbuffer.pixels().to_vec(),
        })
    }

    fn show_background_snapshot(&mut self) -> bool {
        let Some(snapshot) = self.background_snapshot.as_ref() else {
            return false;
        };
        let Some(window) = read_window_state(self.hwnd) else {
            return false;
        };
        if snapshot.width != window.width || snapshot.height != window.height {
            return false;
        }
        let screen_dc = unsafe { GetDC(None) };
        if screen_dc.is_invalid() {
            return false;
        }
        if self.backbuffer.is_none()
            || self.backbuffer.as_ref().is_some_and(|buffer| {
                buffer.width() != window.width || buffer.height() != window.height
            })
        {
            self.backbuffer = LayeredBackbuffer::new(screen_dc, window.width, window.height);
        }
        let applied = if let Some(backbuffer) = self.backbuffer.as_mut() {
            if !backbuffer.copy_pixels_from(&snapshot.bgra) {
                backbuffer.invalidate();
                false
            } else {
                match update_layered_window(self.hwnd, screen_dc, backbuffer, window) {
                    Ok(()) => {
                        backbuffer.mark_valid();
                        true
                    }
                    Err(error) => {
                        backbuffer.invalidate();
                        (self.config.report_error)(
                            "failed to restore layered auxiliary window snapshot",
                            &error,
                        );
                        false
                    }
                }
            }
        } else {
            false
        };
        unsafe {
            let _ = ReleaseDC(None, screen_dc);
        }
        applied
    }

    fn track_mouse_leave(&mut self, hwnd: HWND) {
        if self.mouse_tracking {
            return;
        }
        let mut event = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            if TrackMouseEvent(&mut event).is_ok() {
                self.mouse_tracking = true;
            }
        }
    }

    fn is_titlebar_drag_area(&self, hwnd: HWND, lparam: LPARAM) -> bool {
        let Some(drag_height) = self.config.titlebar_drag_height else {
            return false;
        };
        let point = self
            .dpi_context
            .map(|context| context.logical_point(screen_point_to_client(hwnd, lparam)))
            .unwrap_or_else(|| screen_point_to_client(hwnd, lparam));
        let Some(window) = read_window_state(hwnd) else {
            return false;
        };
        if point.y < 0 || point.y >= drag_height {
            return false;
        }
        if let Some(exclusion_rect) = self.config.drag_exclusion_rect {
            let logical_size = self
                .dpi_context
                .map(|context| {
                    context
                        .scale
                        .logical_size(Size::new(window.width, window.height))
                })
                .unwrap_or(Size::new(window.width, window.height));
            let rect = exclusion_rect(logical_size.width, logical_size.height);
            if point.x >= rect.left
                && point.x < rect.right
                && point.y >= rect.top
                && point.y < rect.bottom
            {
                return false;
            }
        }
        true
    }
}

fn read_window_state(hwnd: HWND) -> Option<LayeredWindowState> {
    let mut client_rect = windows::Win32::Foundation::RECT::default();
    let mut window_rect = windows::Win32::Foundation::RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client_rect);
        let _ = GetWindowRect(hwnd, &mut window_rect);
    }
    let (width, height) = rect_size(client_rect);
    Some(LayeredWindowState {
        window_rect,
        width,
        height,
    })
}

#[derive(Clone, Copy)]
struct LayeredWindowState {
    window_rect: windows::Win32::Foundation::RECT,
    width: i32,
    height: i32,
}

fn update_layered_window(
    hwnd: HWND,
    screen_dc: windows::Win32::Graphics::Gdi::HDC,
    backbuffer: &LayeredBackbuffer,
    window: LayeredWindowState,
) -> windows::core::Result<()> {
    let dest = WinPoint {
        x: window.window_rect.left,
        y: window.window_rect.top,
    };
    let size = SIZE {
        cx: backbuffer.width(),
        cy: backbuffer.height(),
    };
    let source = WinPoint { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    unsafe {
        UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            Some(&dest),
            Some(&size),
            Some(backbuffer.hdc()),
            Some(&source),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
    }
}

fn register_layered_window_class(class_name: &str, wnd_proc: WNDPROC) -> Result<(), Error> {
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
    let class_name = widestring(class_name);
    let cursor = unsafe { LoadCursorW(Some(HINSTANCE::default()), IDC_ARROW) }?;
    let wcx = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: wnd_proc,
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    let atom = unsafe { RegisterClassExW(&wcx) };
    if atom == 0 {
        let error = Error::from_thread();
        if error.code().0 as u32 != 0x00000582 {
            return Err(error);
        }
    }
    Ok(())
}

fn apply_layered_window_chrome(hwnd: HWND) {
    unsafe {
        let corner_preference = DWM_WINDOW_CORNER_PREFERENCE(DWMWCP_ROUND.0);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

fn set_arrow_cursor() {
    if let Ok(cursor) = unsafe { LoadCursorW(Some(HINSTANCE::default()), IDC_ARROW) } {
        unsafe {
            let _ = SetCursor(Some(cursor));
        }
    }
}

fn unpack_point(lparam: LPARAM) -> Point {
    let x = (lparam.0 & 0xFFFF) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    Point::new(x, y)
}

fn screen_point_to_client(hwnd: HWND, lparam: LPARAM) -> Point {
    let mut point = WinPoint {
        x: (lparam.0 & 0xFFFF) as i16 as i32,
        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
    };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut point);
    }
    Point::new(point.x, point.y)
}

fn wheel_delta(wparam: WPARAM) -> i32 {
    ((wparam.0 >> 16) & 0xFFFF) as i16 as i32
}

fn wheel_scroll_units(delta: i32) -> i32 {
    if delta == 0 {
        return 0;
    }
    let direction = if delta > 0 { -1 } else { 1 };
    let units = (delta.abs() / WHEEL_DELTA as i32).max(1);
    direction * units
}

fn widestring(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
