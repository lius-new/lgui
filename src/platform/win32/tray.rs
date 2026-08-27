use std::{
    cell::RefCell,
    io,
    mem::size_of,
    sync::{mpsc, Arc, OnceLock},
    thread::{self, JoinHandle},
};

use windows::{
    core::{w, PCWSTR, PWSTR},
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{DeleteObject, HBITMAP},
        System::LibraryLoader::GetModuleHandleW,
        UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        UI::Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD,
            NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
        },
        UI::WindowsAndMessaging::{
            CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
            DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, InsertMenuItemW,
            LoadCursorW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassExW,
            RegisterWindowMessageW, SetForegroundWindow, SetWindowPos, ShowWindowAsync,
            TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HICON,
            HWND_TOPMOST, IDC_ARROW, IDI_APPLICATION, MENUITEMINFOW, MFS_CHECKED, MFS_DISABLED,
            MFS_ENABLED, MFT_SEPARATOR, MFT_STRING, MIIM_BITMAP, MIIM_FTYPE, MIIM_ID, MIIM_STATE,
            MIIM_STRING, MSG, SM_CXSMICON, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE,
            SW_SHOW, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_DESTROY,
            WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
};

use crate::{
    application::{ApplicationContext, TrayAction, TrayRegistration},
    platform::TrayMenuEntry,
};

use super::ico::create_icon_from_ico_bytes;

#[cfg(feature = "svg")]
use crate::core::{Color, IconStyle, UiRect};
#[cfg(feature = "svg")]
use std::{ffi::c_void, ptr::null_mut};
#[cfg(feature = "svg")]
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, GetSysColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, COLOR_MENUTEXT,
    DIB_RGB_COLORS,
};

pub const TRAY_MESSAGE_ID: u32 = WM_APP + 1;
const DEFAULT_TRAY_ICON_ID: u32 = 1;
const TRAY_HOST_CLASS: &str = "LguiIndependentTrayHost";
const MENU_COMMAND_BASE: u32 = 1;

type VisibilitySync = Arc<dyn Fn(bool) + Send + Sync + 'static>;

thread_local! {
    static TRAY_HOST_STATE: RefCell<Option<TrayHostState>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
pub struct TrayIconHandle {
    hwnd: HWND,
    id: u32,
}

impl TrayIconHandle {
    pub const fn hwnd(self) -> HWND {
        self.hwnd
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub fn show_notification(self, title: &str, body: &str) -> io::Result<()> {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, body);
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

pub struct Win32TrayIcon {
    hwnd: HWND,
    icon: HICON,
    owns_icon: bool,
    tooltip: String,
    id: u32,
    installed: bool,
}

impl Win32TrayIcon {
    pub fn new(hwnd: HWND, icon: HICON, tooltip: impl Into<String>) -> Self {
        Self {
            hwnd,
            icon,
            owns_icon: false,
            tooltip: tooltip.into(),
            id: DEFAULT_TRAY_ICON_ID,
            installed: false,
        }
    }

    pub fn from_ico_bytes(
        hwnd: HWND,
        bytes: &'static [u8],
        size: i32,
        tooltip: impl Into<String>,
    ) -> io::Result<Self> {
        let icon = create_icon_from_ico_bytes(bytes, size, size)?;
        let mut tray = Self::new(hwnd, icon, tooltip);
        tray.owns_icon = true;
        Ok(tray)
    }

    pub fn install(&mut self) -> io::Result<()> {
        let mut data = self.data();
        copy_wide(&mut data.szTip, &self.tooltip);
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            self.installed = true;
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn restore(&mut self) -> io::Result<()> {
        self.install()
    }

    pub fn remove(&mut self) {
        if !self.installed {
            return;
        }
        let data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            ..Default::default()
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        self.installed = false;
    }

    pub const fn handle(&self) -> TrayIconHandle {
        TrayIconHandle {
            hwnd: self.hwnd,
            id: self.id,
        }
    }

    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: self.id,
            uFlags: NIF_MESSAGE | NIF_TIP | NIF_ICON,
            uCallbackMessage: TRAY_MESSAGE_ID,
            hIcon: self.icon,
            ..Default::default()
        }
    }
}

impl Drop for Win32TrayIcon {
    fn drop(&mut self) {
        self.remove();
        if self.owns_icon {
            unsafe {
                let _ = DestroyIcon(self.icon);
            }
        }
    }
}

/// Owns the notification icon and its Win32 message loop on a thread that never renders the
/// application. A blocked renderer therefore cannot prevent the tray menu from opening.
pub(crate) struct Win32TrayHost {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl Win32TrayHost {
    pub(crate) fn spawn(
        registration: Arc<TrayRegistration>,
        context: ApplicationContext,
        main_hwnd: HWND,
        sync_visibility: impl Fn(bool) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let main_hwnd = main_hwnd.0 as isize;
        let sync_visibility: VisibilitySync = Arc::new(sync_visibility);
        let thread = thread::Builder::new()
            .name("lgui-tray".to_string())
            .spawn(move || {
                run_tray_thread(registration, context, main_hwnd, sync_visibility, ready_tx);
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
    registration: Arc<TrayRegistration>,
    context: ApplicationContext,
    main_hwnd: isize,
    sync_visibility: VisibilitySync,
}

fn run_tray_thread(
    registration: Arc<TrayRegistration>,
    context: ApplicationContext,
    main_hwnd: isize,
    sync_visibility: VisibilitySync,
    ready: mpsc::SyncSender<Result<isize, String>>,
) {
    let initialized = initialize_tray_host(registration, context, main_hwnd, sync_visibility);
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

fn initialize_tray_host(
    registration: Arc<TrayRegistration>,
    context: ApplicationContext,
    main_hwnd: isize,
    sync_visibility: VisibilitySync,
) -> io::Result<HWND> {
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

    let tooltip = registration.options.tooltip.clone();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let icon_size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.max(16);
    let mut icon = if let Some(bytes) = registration.options.icon_bytes {
        Win32TrayIcon::from_ico_bytes(hwnd, bytes, icon_size, tooltip)?
    } else {
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION) }.map_err(windows_io_error)?;
        Win32TrayIcon::new(hwnd, icon, tooltip)
    };
    icon.install()?;
    TRAY_HOST_STATE.with(|state| {
        *state.borrow_mut() = Some(TrayHostState {
            icon,
            registration,
            context,
            main_hwnd,
            sync_visibility,
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
                        .and_then(|state| state.registration.options.activate.clone())
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
                        .map(|state| state.registration.options.items.clone())
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

fn is_activation_message(message: u32) -> bool {
    message == WM_LBUTTONUP
}

fn execute_action(action: TrayAction) {
    let state = TRAY_HOST_STATE.with(|state| {
        state.borrow().as_ref().map(|state| {
            (
                state.main_hwnd,
                Arc::clone(&state.sync_visibility),
                Arc::clone(&state.registration.handler),
                state.context.clone(),
            )
        })
    });
    let Some((main_hwnd, sync_visibility, handler, context)) = state else {
        return;
    };
    let main_hwnd = HWND(main_hwnd as _);
    match action {
        TrayAction::ShowMainWindow => {
            unsafe {
                let _ = ShowWindowAsync(main_hwnd, SW_SHOW);
                let _ = SetForegroundWindow(main_hwnd);
            }
            sync_visibility(true);
        }
        TrayAction::HideMainWindow => {
            unsafe {
                let _ = ShowWindowAsync(main_hwnd, SW_HIDE);
            }
            sync_visibility(false);
        }
        TrayAction::Exit => {
            context.windows().exit();
        }
        TrayAction::Command {
            name,
            show_main_window,
        } => {
            handler(&context, &name);
            if show_main_window {
                unsafe {
                    let _ = ShowWindowAsync(main_hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(main_hwnd);
                }
                sync_visibility(true);
            }
        }
    }
}

fn show_tray_menu(hwnd: HWND, items: &[TrayMenuEntry<TrayAction>]) -> Option<TrayAction> {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            point.x,
            point.y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
        );
    }
    let menu = NativeTrayMenu::new(hwnd, items).ok()?;
    let selected = unsafe {
        let _ = SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            menu.handle,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        selected.0 as u32
    };
    menu.action(selected)
}

struct NativeTrayMenu {
    handle: windows::Win32::UI::WindowsAndMessaging::HMENU,
    actions: Vec<(u32, TrayAction)>,
    bitmaps: Vec<HBITMAP>,
}

impl NativeTrayMenu {
    fn new(hwnd: HWND, entries: &[TrayMenuEntry<TrayAction>]) -> io::Result<Self> {
        let handle = unsafe { CreatePopupMenu() }.map_err(windows_io_error)?;
        let mut menu = Self {
            handle,
            actions: Vec::new(),
            bitmaps: Vec::new(),
        };
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let icon_size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.max(16);
        for (position, entry) in entries.iter().enumerate() {
            match entry {
                TrayMenuEntry::Separator => {
                    let info = MENUITEMINFOW {
                        cbSize: size_of::<MENUITEMINFOW>() as u32,
                        fMask: MIIM_FTYPE,
                        fType: MFT_SEPARATOR,
                        ..Default::default()
                    };
                    unsafe { InsertMenuItemW(handle, position as u32, true, &info) }
                        .map_err(windows_io_error)?;
                }
                TrayMenuEntry::Item(item) => {
                    let command_id = MENU_COMMAND_BASE + menu.actions.len() as u32;
                    let mut label = wide(&item.label);
                    let mut state = if item.enabled {
                        MFS_ENABLED
                    } else {
                        MFS_DISABLED
                    };
                    if item.checked {
                        state |= MFS_CHECKED;
                    }
                    let bitmap = item
                        .icon
                        .and_then(|icon| create_menu_bitmap(icon, icon_size).ok());
                    let mut mask = MIIM_FTYPE | MIIM_ID | MIIM_STRING | MIIM_STATE;
                    if bitmap.is_some() {
                        mask |= MIIM_BITMAP;
                    }
                    let info = MENUITEMINFOW {
                        cbSize: size_of::<MENUITEMINFOW>() as u32,
                        fMask: mask,
                        fType: MFT_STRING,
                        fState: state,
                        wID: command_id,
                        dwTypeData: PWSTR(label.as_mut_ptr()),
                        cch: label.len().saturating_sub(1) as u32,
                        hbmpItem: bitmap.unwrap_or_default(),
                        ..Default::default()
                    };
                    unsafe { InsertMenuItemW(handle, position as u32, true, &info) }
                        .map_err(windows_io_error)?;
                    if let Some(bitmap) = bitmap {
                        menu.bitmaps.push(bitmap);
                    }
                    menu.actions.push((command_id, item.command.clone()));
                }
            }
        }
        Ok(menu)
    }

    fn action(&self, command_id: u32) -> Option<TrayAction> {
        self.actions
            .iter()
            .find(|(id, _)| *id == command_id)
            .map(|(_, action)| action.clone())
    }
}

impl Drop for NativeTrayMenu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.handle);
            for bitmap in self.bitmaps.drain(..) {
                let _ = DeleteObject(bitmap.into());
            }
        }
    }
}

#[cfg(feature = "svg")]
fn create_menu_bitmap(icon: &'static str, size: i32) -> io::Result<HBITMAP> {
    let system_color = unsafe { GetSysColor(COLOR_MENUTEXT) };
    let rgb =
        ((system_color & 0xFF) << 16) | (system_color & 0x00FF00) | ((system_color >> 16) & 0xFF);
    let rect = UiRect::new(0, 0, size, size);
    let bitmap = super::rasterize_svg_icon_bgra(icon, rect, IconStyle::new(Color(rgb)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unknown tray menu icon"))?;
    create_bgra_bitmap(bitmap.width, bitmap.height, &bitmap.premultiplied_bgra)
}

#[cfg(not(feature = "svg"))]
fn create_menu_bitmap(_icon: &'static str, _size: i32) -> io::Result<HBITMAP> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "tray menu icons require the svg feature",
    ))
}

#[cfg(feature = "svg")]
fn create_bgra_bitmap(width: i32, height: i32, pixels: &[u8]) -> io::Result<HBITMAP> {
    let expected = width.max(0) as usize * height.max(0) as usize * 4;
    if pixels.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid tray menu bitmap length",
        ));
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = null_mut();
    let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(windows_io_error)?;
    if bits.is_null() {
        unsafe {
            let _ = DeleteObject(bitmap.into());
        }
        return Err(io::Error::last_os_error());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast(), pixels.len());
    }
    Ok(bitmap)
}

fn windows_io_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

fn copy_wide(buffer: &mut [u16], value: &str) {
    buffer.fill(0);
    for (index, unit) in value
        .encode_utf16()
        .take(buffer.len().saturating_sub(1))
        .enumerate()
    {
        buffer[index] = unit;
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_wide, is_activation_message, WM_LBUTTONUP};

    #[test]
    fn single_left_click_activates_the_tray_icon() {
        assert!(is_activation_message(WM_LBUTTONUP));
        assert!(!is_activation_message(
            windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK
        ));
    }

    #[test]
    fn notification_text_is_terminated_and_truncated_to_fit() {
        let mut buffer = [9_u16; 4];
        copy_wide(&mut buffer, "ABCDE");
        assert_eq!(buffer, ['A' as u16, 'B' as u16, 'C' as u16, 0]);
    }
}
