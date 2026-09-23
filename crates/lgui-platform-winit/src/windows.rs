use std::{ffi::c_void, sync::OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    dpi::PhysicalSize,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct NcCalcSizeParams {
    rects: [Rect; 3],
    window_pos: *mut c_void,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
struct NativePoint {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MinMaxInfo {
    reserved: NativePoint,
    max_size: NativePoint,
    max_position: NativePoint,
    min_track_size: NativePoint,
    max_track_size: NativePoint,
}

#[repr(C)]
struct OsVersionInfo {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}

struct ResizeFrameData {
    has_minimum_size: bool,
    has_maximum_size: bool,
}

const WM_NCLBUTTONDOWN: u32 = 0x00A1;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;
const WM_NCDESTROY: u32 = 0x0082;
const WM_GETMINMAXINFO: u32 = 0x0024;
const HTCAPTION: usize = 2;
const HTLEFT: usize = 10;
const HTRIGHT: usize = 11;
const HTTOP: usize = 12;
const HTTOPLEFT: usize = 13;
const HTTOPRIGHT: usize = 14;
const HTBOTTOM: usize = 15;
const HTBOTTOMLEFT: usize = 16;
const HTBOTTOMRIGHT: usize = 17;
const GWL_STYLE: i32 = -16;
const WS_THICKFRAME: isize = 0x0004_0000;
const WS_MAXIMIZE: isize = 0x0100_0000;
const SM_CXFRAME: i32 = 32;
const SM_CYFRAME: i32 = 33;
const SM_CXPADDEDBORDER: i32 = 92;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const RESIZE_FRAME_SUBCLASS_ID: usize = 0x4C47_5549;

type SubclassProc =
    Option<unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize>;

#[link(name = "user32")]
unsafe extern "system" {
    fn ClientToScreen(hwnd: isize, point: *mut NativePoint) -> i32;
    fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn GetSystemMetricsForDpi(index: i32, dpi: u32) -> i32;
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn IsZoomed(hwnd: isize) -> i32;
    fn ReleaseCapture() -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn SetWindowPos(
        hwnd: isize,
        insert_after: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn DefSubclassProc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn SetWindowSubclass(hwnd: isize, proc: SubclassProc, id: usize, reference_data: usize) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(version: *mut OsVersionInfo) -> i32;
}

/// Restores the transparent native resize frame that winit removes when it
/// expands an undecorated window's client area over the whole `WS_THICKFRAME`.
///
/// The left, right and bottom strips are outside the visible client surface,
/// matching native Windows applications. Windows 10 cannot retain a top
/// non-client inset without drawing a caption, so the top edge remains a
/// small inside hit target there.
pub(crate) fn install_outer_resize_frame(
    window: &Window,
    has_minimum_size: bool,
    has_maximum_size: bool,
) {
    let Some(hwnd) = window_hwnd(window) else {
        return;
    };
    let mut client = Rect::default();
    if unsafe { GetClientRect(hwnd, &mut client) } == 0 {
        return;
    }
    let data = Box::new(ResizeFrameData {
        has_minimum_size,
        has_maximum_size,
    });
    let data = Box::into_raw(data);
    if unsafe {
        SetWindowSubclass(
            hwnd,
            Some(resize_frame_subclass),
            RESIZE_FRAME_SUBCLASS_ID,
            data as usize,
        )
    } == 0
    {
        unsafe { drop(Box::from_raw(data)) };
        return;
    }

    let insets = resize_frame_insets(hwnd);
    let width = client
        .right
        .saturating_sub(client.left)
        .saturating_add(insets.left)
        .saturating_add(insets.right);
    let height = client
        .bottom
        .saturating_sub(client.top)
        .saturating_add(insets.top)
        .saturating_add(insets.bottom);
    unsafe {
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

pub(crate) fn request_frameless_inner_size(window: &Window, size: PhysicalSize<u32>) {
    let Some(hwnd) = window_hwnd(window) else {
        let _ = window.request_inner_size(size);
        return;
    };
    let insets = resize_frame_insets(hwnd);
    let width = (size.width as i32)
        .saturating_add(insets.left)
        .saturating_add(insets.right);
    let height = (size.height as i32)
        .saturating_add(insets.top)
        .saturating_add(insets.bottom);
    unsafe {
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub(crate) fn client_point_to_screen(
    window: &Window,
    point: winit::dpi::PhysicalPosition<i32>,
) -> Option<winit::dpi::PhysicalPosition<i32>> {
    let hwnd = window_hwnd(window)?;
    let mut native = NativePoint {
        x: point.x,
        y: point.y,
    };
    (unsafe { ClientToScreen(hwnd, &mut native) } != 0)
        .then_some(winit::dpi::PhysicalPosition::new(native.x, native.y))
}

unsafe extern "system" fn resize_frame_subclass(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    reference_data: usize,
) -> isize {
    let data = unsafe { &*(reference_data as *const ResizeFrameData) };
    match msg {
        WM_NCCALCSIZE if wparam != 0 && resize_frame_is_active(hwnd) => {
            let params = unsafe { &mut *(lparam as *mut NcCalcSizeParams) };
            let insets = resize_frame_insets(hwnd);
            params.rects[0].left += insets.left;
            params.rects[0].top += insets.top;
            params.rects[0].right -= insets.right;
            params.rects[0].bottom -= insets.bottom;
            0
        }
        WM_NCHITTEST if resize_frame_is_active(hwnd) => {
            let mut rect = Rect::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
                let x = lparam as u16 as i16 as i32;
                let y = (lparam >> 16) as u16 as i16 as i32;
                let insets = resize_frame_insets(hwnd);
                if let Some(hit) = resize_hit_test(rect, x, y, insets.right, insets.bottom) {
                    return hit as isize;
                }
            }
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_GETMINMAXINFO => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            if resize_frame_is_active(hwnd) {
                let info = unsafe { &mut *(lparam as *mut MinMaxInfo) };
                let insets = resize_frame_insets(hwnd);
                let horizontal = insets.left + insets.right;
                let vertical = insets.top + insets.bottom;
                if data.has_minimum_size {
                    info.min_track_size.x = info.min_track_size.x.saturating_add(horizontal);
                    info.min_track_size.y = info.min_track_size.y.saturating_add(vertical);
                }
                if data.has_maximum_size {
                    info.max_track_size.x = info.max_track_size.x.saturating_add(horizontal);
                    info.max_track_size.y = info.max_track_size.y.saturating_add(vertical);
                }
            }
            result
        }
        WM_NCDESTROY => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            unsafe { drop(Box::from_raw(reference_data as *mut ResizeFrameData)) };
            result
        }
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

fn window_hwnd(window: &Window) -> Option<isize> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get())
}

fn resize_frame_is_active(hwnd: isize) -> bool {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    style & WS_THICKFRAME != 0 && style & WS_MAXIMIZE == 0 && unsafe { IsZoomed(hwnd) } == 0
}

fn resize_frame_insets(hwnd: isize) -> Rect {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let padding = unsafe { GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) }.max(0);
    let horizontal = unsafe { GetSystemMetricsForDpi(SM_CXFRAME, dpi) }
        .saturating_add(padding)
        .max(1);
    let vertical = unsafe { GetSystemMetricsForDpi(SM_CYFRAME, dpi) }
        .saturating_add(padding)
        .max(1);
    let top = if is_windows_11_or_newer() {
        ((dpi as f32 / 96.0).round() as i32).max(1)
    } else {
        0
    };
    Rect {
        left: horizontal,
        top,
        right: horizontal,
        bottom: vertical,
    }
}

fn is_windows_11_or_newer() -> bool {
    static WINDOWS_11_OR_NEWER: OnceLock<bool> = OnceLock::new();
    *WINDOWS_11_OR_NEWER.get_or_init(|| {
        let mut version = OsVersionInfo {
            size: std::mem::size_of::<OsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service_pack: [0; 128],
        };
        (unsafe { RtlGetVersion(&mut version) }) == 0 && version.build >= 22_000
    })
}

fn resize_hit_test(rect: Rect, x: i32, y: i32, border_x: i32, border_y: i32) -> Option<usize> {
    let left = x >= rect.left && x < rect.left.saturating_add(border_x);
    let right = x < rect.right && x >= rect.right.saturating_sub(border_x);
    let top = y >= rect.top && y < rect.top.saturating_add(border_y);
    let bottom = y < rect.bottom && y >= rect.bottom.saturating_sub(border_y);
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(HTTOPLEFT),
        (_, true, true, _) => Some(HTTOPRIGHT),
        (true, _, _, true) => Some(HTBOTTOMLEFT),
        (_, true, _, true) => Some(HTBOTTOMRIGHT),
        (true, _, _, _) => Some(HTLEFT),
        (_, true, _, _) => Some(HTRIGHT),
        (_, _, true, _) => Some(HTTOP),
        (_, _, _, true) => Some(HTBOTTOM),
        _ => None,
    }
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
    outer_resize_frame: bool,
) -> WindowAttributes {
    attributes
        .with_undecorated_shadow(outer_resize_frame)
        .with_corner_preference(corner_preference(radius, mode))
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
