use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    platform::windows::{CornerPreference, WindowAttributesExtWindows, WindowExtWindows},
    window::{Window, WindowAttributes},
};

use lgui_core::window::WindowMode;

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
