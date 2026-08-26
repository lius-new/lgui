use std::{io, sync::OnceLock};

use windows::{
    core::w,
    Win32::{
        Foundation::HWND,
        UI::Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD,
            NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
        },
        UI::WindowsAndMessaging::{RegisterWindowMessageW, HICON, WM_APP},
    },
};

pub const TRAY_MESSAGE_ID: u32 = WM_APP + 1;
const DEFAULT_TRAY_ICON_ID: u32 = 1;

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

#[derive(Clone)]
pub struct Win32TrayIcon {
    hwnd: HWND,
    icon: HICON,
    tooltip: String,
    id: u32,
    installed: bool,
}

impl Win32TrayIcon {
    pub fn new(hwnd: HWND, icon: HICON, tooltip: impl Into<String>) -> Self {
        Self {
            hwnd,
            icon,
            tooltip: tooltip.into(),
            id: DEFAULT_TRAY_ICON_ID,
            installed: false,
        }
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
    }
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
    use super::copy_wide;

    #[test]
    fn notification_text_is_terminated_and_truncated_to_fit() {
        let mut buffer = [9_u16; 4];
        copy_wide(&mut buffer, "ABCDE");
        assert_eq!(buffer, ['A' as u16, 'B' as u16, 'C' as u16, 0]);
    }
}
