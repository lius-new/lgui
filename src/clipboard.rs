use crate::platform::ClipboardError;

pub fn read_text() -> Result<Option<String>, ClipboardError> {
    #[cfg(all(feature = "backend-win32", target_os = "windows"))]
    {
        return crate::platform::win32::read_clipboard_text();
    }

    #[cfg(not(all(feature = "backend-win32", target_os = "windows")))]
    Err(ClipboardError::Unavailable)
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    #[cfg(all(feature = "backend-win32", target_os = "windows"))]
    {
        return crate::platform::win32::write_clipboard_text(text);
    }

    #[cfg(not(all(feature = "backend-win32", target_os = "windows")))]
    {
        let _ = text;
        Err(ClipboardError::Unavailable)
    }
}
