use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use winit::window::Window;

pub(crate) fn hwnd(window: &Window) -> Result<HWND, String> {
    let handle = window
        .window_handle()
        .map_err(|error| format!("read native window handle: {error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("winit did not provide a Win32 window handle".to_owned());
    };
    Ok(HWND(handle.hwnd.get() as *mut _))
}
