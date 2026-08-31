use super::{Clipboard, ClipboardError, ClipboardHandle};

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(map_error)?;
        match clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(map_error(error)),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(map_error)?;
        clipboard.set_text(text.to_owned()).map_err(map_error)
    }
}

pub fn system_clipboard() -> ClipboardHandle {
    std::sync::Arc::new(SystemClipboard)
}

pub fn read_text() -> Result<Option<String>, ClipboardError> {
    SystemClipboard.read_text()
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    SystemClipboard.write_text(text)
}

fn map_error(error: arboard::Error) -> ClipboardError {
    match error {
        arboard::Error::ClipboardNotSupported => ClipboardError::Unavailable,
        arboard::Error::ContentNotAvailable => ClipboardError::Unavailable,
        _ => ClipboardError::AccessDenied,
    }
}
