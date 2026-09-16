use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable,
    AccessDenied,
    AllocationFailed,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "clipboard text is unavailable",
            Self::AccessDenied => "clipboard access was denied",
            Self::AllocationFailed => "clipboard allocation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ClipboardError {}

pub trait Clipboard: Send + Sync + 'static {
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;
}

pub type ClipboardHandle = Arc<dyn Clipboard>;
