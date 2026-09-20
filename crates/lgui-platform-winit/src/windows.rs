use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    platform::windows::{CornerPreference, WindowAttributesExtWindows, WindowExtWindows},
    window::{ResizeDirection, Window, WindowAttributes},
};

use lgui_core::window::WindowMode;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Points {
    x: i16,
    y: i16,
}

const WM_NCLBUTTONDOWN: u32 = 0x00A1;
const HTCAPTION: usize = 2;
const HTLEFT: usize = 10;
const HTRIGHT: usize = 11;
const HTTOP: usize = 12;
const HTTOPLEFT: usize = 13;
const HTTOPRIGHT: usize = 14;
const HTBOTTOM: usize = 15;
const HTBOTTOMLEFT: usize = 16;
const HTBOTTOMRIGHT: usize = 17;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetCursorPos(point: *mut Point) -> i32;
    fn ReleaseCapture() -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
}

/// Begin an OS window move via a synthetic `WM_NCLBUTTONDOWN(HTCAPTION)`,
/// bypassing winit's `drag_window`. winit tracks an internal `dragging` flag
/// that it only clears on `WM_EXITSIZEMOVE`; Windows skips that message when
/// a maximized window is restored by dragging its edge, so the flag stays set
/// and winit silently drops every later drag request. Driving the native
/// hit-test message directly avoids that state entirely.
pub(crate) fn begin_os_move(window: &Window) {
    begin_os_drag_with_hit_test(window, HTCAPTION);
}

/// Begin an OS resize via `WM_NCLBUTTONDOWN` with the hit-test code matching
/// the resize direction. See [`begin_os_move`] for why we bypass winit.
pub(crate) fn begin_os_resize(window: &Window, direction: ResizeDirection) {
    begin_os_drag_with_hit_test(window, hit_test_for_resize(direction));
}

fn begin_os_drag_with_hit_test(window: &Window, wparam: usize) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get();
    unsafe {
        let mut pos = Point { x: 0, y: 0 };
        GetCursorPos(&mut pos);
        let points = Points {
            x: pos.x as i16,
            y: pos.y as i16,
        };
        ReleaseCapture();
        // WM_NCLBUTTONDOWN's lParam carries the cursor screen position
        // packed into one value (low word x, high word y).
        let lparam = ((points.y as u16 as usize) << 16) | (points.x as u16 as usize);
        PostMessageW(hwnd, WM_NCLBUTTONDOWN, wparam, lparam as isize);
    }
}

fn hit_test_for_resize(direction: ResizeDirection) -> usize {
    match direction {
        ResizeDirection::North => HTTOP,
        ResizeDirection::South => HTBOTTOM,
        ResizeDirection::East => HTRIGHT,
        ResizeDirection::West => HTLEFT,
        ResizeDirection::NorthEast => HTTOPRIGHT,
        ResizeDirection::NorthWest => HTTOPLEFT,
        ResizeDirection::SouthEast => HTBOTTOMRIGHT,
        ResizeDirection::SouthWest => HTBOTTOMLEFT,
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

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GdiFlush() -> i32;
}

/// Flush any pending GDI output. softbuffer's `present` blits the pixel
/// buffer with `BitBlt`; without a flush, DWM can show stale content right
/// after a resize/restore transition.
pub(crate) fn flush_gdi() {
    unsafe {
        GdiFlush();
    }
}

#[cfg(test)]
#[path = "windows_test.rs"]
mod tests;
