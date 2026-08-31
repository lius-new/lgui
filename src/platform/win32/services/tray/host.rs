use std::{
    cell::RefCell,
    io,
    mem::size_of,
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
                LoadCursorW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassExW,
                TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW,
                IDI_APPLICATION, MSG, SM_CXSMICON, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP,
                WM_RBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::services::{TrayAction, TrayOptions};

use super::{
    icon::Win32TrayIcon,
    menu::show_tray_menu,
    support::{taskbar_created_message, wide, windows_io_error},
    TRAY_MESSAGE_ID,
};

const TRAY_HOST_CLASS: &str = "LguiIndependentTrayHost";

type ActionDispatch = Arc<dyn Fn(TrayAction) + Send + Sync + 'static>;

thread_local! {
    static TRAY_HOST_STATE: RefCell<Option<TrayHostState>> = const { RefCell::new(None) };
}

/// Owns the notification icon and its Win32 message loop on a thread that never renders the
/// application. A blocked renderer therefore cannot prevent the tray menu from opening.
pub(crate) struct Win32TrayHost {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl Win32TrayHost {
    pub(crate) fn spawn(
        options: TrayOptions,
        dispatch: impl Fn(TrayAction) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let dispatch: ActionDispatch = Arc::new(dispatch);
        let thread = thread::Builder::new()
            .name("lgui-tray".to_string())
            .spawn(move || {
                run_tray_thread(options, dispatch, ready_tx);
            })?;
        match ready_rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(io::Error::other(error))
            }
            Err(_) => {
                let _ = thread.join();
                Err(io::Error::other("tray thread stopped during startup"))
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        if self.hwnd != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(self.hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            self.hwnd = 0;
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Win32TrayHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct TrayHostState {
    icon: Win32TrayIcon,
    options: TrayOptions,
    dispatch: ActionDispatch,
}

fn run_tray_thread(
    options: TrayOptions,
    dispatch: ActionDispatch,
    ready: mpsc::SyncSender<Result<isize, String>>,
) {
    let initialized = initialize_tray_host(options, dispatch);
    match initialized {
        Ok(hwnd) => {
            let _ = ready.send(Ok(hwnd.0 as isize));
        }
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    TRAY_HOST_STATE.with(|state| *state.borrow_mut() = None);
}

fn initialize_tray_host(options: TrayOptions, dispatch: ActionDispatch) -> io::Result<HWND> {
    let instance = HINSTANCE(
        unsafe { GetModuleHandleW(None) }
            .map_err(windows_io_error)?
            .0,
    );
    let class_name = wide(TRAY_HOST_CLASS);
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.map_err(windows_io_error)?;
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(tray_host_proc),
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        let error = windows::core::Error::from_thread();
        if error.code().0 as u32 != 0x0000_0582 {
            return Err(windows_io_error(error));
        }
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .map_err(windows_io_error)?;

    let tooltip = options.tooltip.clone();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let icon_size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.max(16);
    let mut icon = if let Some(bytes) = options.icon_bytes {
        Win32TrayIcon::from_ico_bytes(hwnd, bytes, icon_size, tooltip)?
    } else {
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION) }.map_err(windows_io_error)?;
        Win32TrayIcon::new(hwnd, icon, tooltip)
    };
    icon.install()?;
    TRAY_HOST_STATE.with(|state| {
        *state.borrow_mut() = Some(TrayHostState {
            icon,
            options,
            dispatch,
        });
    });
    Ok(hwnd)
}

extern "system" fn tray_host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == TRAY_MESSAGE_ID {
        match lparam.0 as u32 {
            message if is_activation_message(message) => {
                let action = TRAY_HOST_STATE.with(|state| {
                    state
                        .borrow()
                        .as_ref()
                        .and_then(|state| state.options.activate.clone())
                });
                if let Some(action) = action {
                    execute_action(action);
                }
            }
            WM_RBUTTONUP => {
                let items = TRAY_HOST_STATE.with(|state| {
                    state
                        .borrow()
                        .as_ref()
                        .map(|state| state.options.items.clone())
                        .unwrap_or_default()
                });
                if let Some(action) = show_tray_menu(hwnd, &items) {
                    execute_action(action);
                }
            }
            _ => {}
        }
        return LRESULT(0);
    }
    if message == taskbar_created_message() {
        TRAY_HOST_STATE.with(|state| {
            if let Some(state) = state.borrow_mut().as_mut() {
                let _ = state.icon.restore();
            }
        });
        return LRESULT(0);
    }
    match message {
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            TRAY_HOST_STATE.with(|state| *state.borrow_mut() = None);
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub(super) fn is_activation_message(message: u32) -> bool {
    message == WM_LBUTTONUP
}

fn execute_action(action: TrayAction) {
    let dispatch = TRAY_HOST_STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|state| Arc::clone(&state.dispatch))
    });
    if let Some(dispatch) = dispatch {
        dispatch(action);
    }
}
