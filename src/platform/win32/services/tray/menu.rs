use std::{io, mem::size_of};

use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{HWND, POINT, WPARAM},
        Graphics::Gdi::{DeleteObject, HBITMAP},
        UI::{
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            WindowsAndMessaging::{
                CreatePopupMenu, DestroyMenu, GetCursorPos, InsertMenuItemW, PostMessageW,
                SetForegroundWindow, SetWindowPos, TrackPopupMenu, HWND_TOPMOST, MENUITEMINFOW,
                MFS_CHECKED, MFS_DISABLED, MFS_ENABLED, MFT_SEPARATOR, MFT_STRING, MIIM_BITMAP,
                MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, SM_CXSMICON, SWP_NOACTIVATE,
                SWP_NOSIZE, SWP_NOZORDER, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
            },
        },
    },
};

use crate::services::{TrayAction, TrayMenuEntry};

use super::support::{wide, windows_io_error};

#[cfg(feature = "svg")]
use crate::core::{Color, IconStyle, UiRect};
#[cfg(feature = "svg")]
use std::{ffi::c_void, ptr::null_mut};
#[cfg(feature = "svg")]
use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, GetSysColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, COLOR_MENUTEXT,
    DIB_RGB_COLORS,
};

const MENU_COMMAND_BASE: u32 = 1;

pub(super) fn show_tray_menu(
    hwnd: HWND,
    items: &[TrayMenuEntry<TrayAction>],
) -> Option<TrayAction> {
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
        let _ = PostMessageW(
            Some(hwnd),
            WM_NULL,
            WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
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
    let rect = UiRect::new(0.0, 0.0, size as f32, size as f32);
    let bitmap =
        crate::platform::win32::rasterize_svg_icon_bgra(icon, rect, IconStyle::new(Color(rgb)))
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
