use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
    },
};

use crate::platform::{Clipboard, ClipboardError};

const CF_UNICODETEXT_FORMAT: u32 = 13;

pub struct Win32Clipboard;

impl Clipboard for Win32Clipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        read_clipboard_text()
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        write_clipboard_text(text)
    }
}

struct OpenClipboardGuard;

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn open_clipboard() -> Result<OpenClipboardGuard, ClipboardError> {
    unsafe { OpenClipboard(None) }
        .map(|_| OpenClipboardGuard)
        .map_err(|_| ClipboardError::AccessDenied)
}

pub fn write_clipboard_text(text: &str) -> Result<(), ClipboardError> {
    let _guard = open_clipboard()?;
    let wide = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let byte_len = wide.len() * std::mem::size_of::<u16>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) }
        .map_err(|_| ClipboardError::AllocationFailed)?;
    let lock = unsafe { GlobalLock(memory) as *mut u16 };
    if lock.is_null() {
        unsafe {
            let _ = GlobalFree(Some(memory));
        }
        return Err(ClipboardError::AllocationFailed);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), lock, wide.len());
        let _ = GlobalUnlock(memory);
    }
    if unsafe { EmptyClipboard() }.is_err() {
        unsafe {
            let _ = GlobalFree(Some(memory));
        }
        return Err(ClipboardError::AccessDenied);
    }
    if unsafe { SetClipboardData(CF_UNICODETEXT_FORMAT, Some(HANDLE(memory.0))) }.is_err() {
        unsafe {
            let _ = GlobalFree(Some(memory));
        }
        return Err(ClipboardError::AccessDenied);
    }
    Ok(())
}

pub fn read_clipboard_text() -> Result<Option<String>, ClipboardError> {
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT_FORMAT) }.is_err() {
        return Ok(None);
    }
    let _guard = open_clipboard()?;
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT_FORMAT) }
        .map_err(|_| ClipboardError::Unavailable)?;
    let memory = HGLOBAL(handle.0);
    let lock = unsafe { GlobalLock(memory) as *const u16 };
    if lock.is_null() {
        return Err(ClipboardError::Unavailable);
    }
    let mut len = 0usize;
    unsafe {
        while *lock.add(len) != 0 {
            len += 1;
        }
    }
    let text = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(lock, len)) };
    unsafe {
        let _ = GlobalUnlock(memory);
    }
    Ok(Some(text))
}
