use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    platform::windows::{CornerPreference, WindowAttributesExtWindows, WindowExtWindows},
    window::{Window, WindowAttributes},
};

use lgui_core::window::WindowMode;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetClassLongPtrW(hwnd: isize, index: i32) -> usize;
    fn SetClassLongPtrW(hwnd: isize, index: i32, value: isize) -> usize;
}

const GCL_STYLE: i32 = -26;
const CS_DBLCLKS: usize = 0x0008;

/// Windows only generates `WM_NCLBUTTONDBLCLK` (which `DefWindowProc` turns
/// into the standard "double-click titlebar to maximize/restore" behavior)
/// when the window class carries `CS_DBLCLKS`. winit registers its class with
/// only `CS_HREDRAW | CS_VREDRAW`, so a double-click on a borderless window's
/// drag region does nothing. Patch the class style after the window exists.
pub(crate) fn enable_titlebar_double_click(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get();
    unsafe {
        let style = GetClassLongPtrW(hwnd, GCL_STYLE);
        if style & CS_DBLCLKS == 0 {
            SetClassLongPtrW(hwnd, GCL_STYLE, (style | CS_DBLCLKS) as isize);
        }
    }
}

pub(crate) fn with_corner_radius(
    attributes: WindowAttributes,
    radius: i32,
    mode: WindowMode,
) -> WindowAttributes {
    attributes.with_corner_preference(corner_preference(radius, mode))
}

pub(crate) fn set_corner_radius(window: &Window, radius: i32, mode: WindowMode) {
    window.set_corner_preference(corner_preference(radius, mode));
}

fn corner_preference(radius: i32, mode: WindowMode) -> CornerPreference {
    if radius <= 0 || mode != WindowMode::Windowed {
        CornerPreference::DoNotRound
    } else if radius <= 4 {
        CornerPreference::RoundSmall
    } else {
        CornerPreference::Round
    }
}

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

#[cfg(test)]
#[path = "windows_test.rs"]
mod tests;
