use std::{cell::RefCell, mem::size_of, sync::Arc};

use windows::{
    core::{Error, Result, PCWSTR},
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, UpdateWindow, PAINTSTRUCT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetMessageW, LoadCursorW, PostQuitMessage, RegisterClassExW, SetWindowPos,
                ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW,
                MINMAXINFO, MSG, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WM_CHAR,
                WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SIZE, WNDCLASSEXW,
                WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_OVERLAPPEDWINDOW, WS_SYSMENU,
            },
        },
    },
};

use crate::{
    application::{AppView, ApplicationBackend, WindowOptions},
    core::{InputEvent, KeyCode, KeyModifiers, Point, PointerButton, Size, UiRect},
    renderer::RenderBackend,
    session::UiSession,
};

use super::{DpiContext, GdiRenderer};

const WINDOW_CLASS: &str = "LguiApplicationWindow";

thread_local! {
    static STATE: RefCell<Option<WindowState>> = const { RefCell::new(None) };
}

#[derive(Default)]
pub struct Win32Application;

struct WindowState {
    view: AppView,
    session: UiSession,
    renderer: GdiRenderer,
    logical_size: Size,
    minimum_size: Option<Size>,
}

impl ApplicationBackend for Win32Application {
    type Error = Error;

    fn run(self, options: WindowOptions, view: AppView) -> Result<()> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
        let class_name = wide(WINDOW_CLASS);
        let title = wide(&options.title);
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

        STATE.with(|state| {
            *state.borrow_mut() = Some(WindowState {
                view,
                session: UiSession::new(),
                renderer: GdiRenderer::default(),
                logical_size: options.size,
                minimum_size: options.minimum_size,
            });
        });
        let style = if options.resizable {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
        };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                options.size.width,
                options.size.height,
                None,
                None,
                Some(instance),
                None,
            )
        }?;
        install_wake(hwnd);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
        }

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        STATE.with(|state| state.borrow_mut().take());
        Ok(())
    }
}

fn install_wake(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    STATE.with(|state| {
        if let Some(state) = state.borrow().as_ref() {
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
    match message {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            STATE.with(|state| {
                let state = state.borrow();
                let Some((state, minimum)) = state
                    .as_ref()
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
        WM_CHAR => {
            if let Some(character) = char::from_u32(wparam.0 as u32) {
                dispatch_input(hwnd, InputEvent::TextInput(character.to_string()));
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(key) = key_code(wparam.0) {
                dispatch_input(
                    hwnd,
                    InputEvent::KeyDown {
                        key,
                        modifiers: KeyModifiers::default(),
                    },
                );
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let target = unsafe { BeginPaint(hwnd, &mut paint) };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else {
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
        state
            .renderer
            .clear(target, UiRect::new(0, 0, physical.width, physical.height));
        state.renderer.draw_scene(target, &scene, None);
        state.session.runtime().run_effects();
    });
    unsafe {
        let _ = EndPaint(hwnd, &paint);
    }
}

fn dispatch_input(hwnd: HWND, input: InputEvent) {
    STATE.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut() {
            let _ = state.session.handle_input(input);
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
            .as_ref()
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

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
