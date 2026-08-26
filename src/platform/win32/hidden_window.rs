use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    mem::size_of,
    rc::Rc,
};

use windows::{
    core::{Error, Result, PCWSTR},
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
            GetMessageW, KillTimer, PostQuitMessage, RegisterClassExW, SetTimer, TranslateMessage,
            MSG, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_SIZE, WM_TIMER, WNDCLASSEXW,
            WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
};

#[derive(Default)]
struct HiddenWindowCallbacks {
    resize: Option<Rc<dyn Fn()>>,
    timer: Option<Rc<dyn Fn(usize)>>,
    close_requested: Option<Rc<dyn Fn()>>,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<isize, HiddenWindowCallbacks>> = RefCell::new(HashMap::new());
}

#[derive(Clone)]
pub struct Win32HiddenWindow {
    inner: Rc<HiddenWindowInner>,
}

struct HiddenWindowInner {
    hwnd: HWND,
    closed: Cell<bool>,
}

impl Drop for HiddenWindowInner {
    fn drop(&mut self) {
        if !self.closed.replace(true) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

impl Win32HiddenWindow {
    pub fn new(class_name: &str, width: i32, height: i32) -> Result<Self> {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }?.0);
        let class_name = wide(class_name);
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(hidden_window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        unsafe {
            RegisterClassExW(&class);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                -32000,
                -32000,
                width.max(1),
                height.max(1),
                None,
                None,
                Some(instance),
                None,
            )
        }?;
        WINDOWS.with(|windows| {
            windows
                .borrow_mut()
                .insert(hwnd.0 as isize, HiddenWindowCallbacks::default());
        });
        Ok(Self {
            inner: Rc::new(HiddenWindowInner {
                hwnd,
                closed: Cell::new(false),
            }),
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.inner.hwnd
    }

    pub fn client_rect(&self) -> RECT {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.inner.hwnd, &mut rect);
        }
        rect
    }

    pub fn on_resize(&self, handler: impl Fn() + 'static) {
        self.update_callbacks(|callbacks| callbacks.resize = Some(Rc::new(handler)));
    }

    pub fn on_timer(&self, handler: impl Fn(usize) + 'static) {
        self.update_callbacks(|callbacks| callbacks.timer = Some(Rc::new(handler)));
    }

    pub fn on_close_requested(&self, handler: impl Fn() + 'static) {
        self.update_callbacks(|callbacks| callbacks.close_requested = Some(Rc::new(handler)));
    }

    pub fn set_timer(&self, id: usize, interval_ms: u32) -> Result<()> {
        let timer = unsafe { SetTimer(Some(self.inner.hwnd), id, interval_ms.max(1), None) };
        (timer != 0).then_some(()).ok_or_else(Error::from_thread)
    }

    pub fn kill_timer(&self, id: usize) {
        unsafe {
            let _ = KillTimer(Some(self.inner.hwnd), id);
        }
    }

    pub fn run_message_loop(&self) -> Result<()> {
        let mut message = MSG::default();
        loop {
            match unsafe { GetMessageW(&mut message, None, 0, 0).0 } {
                -1 => return Err(Error::from_thread()),
                0 => return Ok(()),
                _ => unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                },
            }
        }
    }

    pub fn close(&self) {
        if !self.inner.closed.replace(true) {
            unsafe {
                let _ = DestroyWindow(self.inner.hwnd);
            }
        }
    }

    fn update_callbacks(&self, update: impl FnOnce(&mut HiddenWindowCallbacks)) {
        WINDOWS.with(|windows| {
            if let Some(callbacks) = windows.borrow_mut().get_mut(&(self.inner.hwnd.0 as isize)) {
                update(callbacks);
            }
        });
    }
}

extern "system" fn hidden_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let callback = WINDOWS.with(|windows| {
        let windows = windows.borrow();
        let callbacks = windows.get(&(hwnd.0 as isize))?;
        match message {
            WM_SIZE => callbacks.resize.clone().map(HiddenCallback::Resize),
            WM_TIMER => callbacks.timer.clone().map(HiddenCallback::Timer),
            WM_CLOSE => callbacks.close_requested.clone().map(HiddenCallback::Close),
            _ => None,
        }
    });
    match callback {
        Some(HiddenCallback::Resize(handler)) => {
            handler();
            LRESULT(0)
        }
        Some(HiddenCallback::Timer(handler)) => {
            handler(wparam.0);
            LRESULT(0)
        }
        Some(HiddenCallback::Close(handler)) => {
            handler();
            LRESULT(0)
        }
        None if message == WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        None if message == WM_DESTROY => {
            WINDOWS.with(|windows| {
                windows.borrow_mut().remove(&(hwnd.0 as isize));
            });
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

enum HiddenCallback {
    Resize(Rc<dyn Fn()>),
    Timer(Rc<dyn Fn(usize)>),
    Close(Rc<dyn Fn()>),
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
