use std::sync::OnceLock;

use windows::{core::w, Win32::UI::WindowsAndMessaging::RegisterWindowMessageW};

pub fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

pub(super) fn copy_wide(buffer: &mut [u16], value: &str) {
    buffer.fill(0);
    for (index, unit) in value
        .encode_utf16()
        .take(buffer.len().saturating_sub(1))
        .enumerate()
    {
        buffer[index] = unit;
    }
}

#[cfg(feature = "backend-winit")]
pub(super) fn windows_io_error(error: windows::core::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

#[cfg(feature = "backend-winit")]
pub(super) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
