use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    platform::windows::WindowAttributesExtWindows,
    window::{Window, WindowAttributes},
};

pub(crate) fn with_owner(
    attributes: WindowAttributes,
    owner: &Window,
) -> Result<WindowAttributes, String> {
    let handle = owner
        .window_handle()
        .map_err(|error| format!("read native owner handle: {error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("winit did not provide a Win32 owner handle".to_owned());
    };
    Ok(attributes.with_owner_window(handle.hwnd.get()))
}
