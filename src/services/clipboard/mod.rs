//! Clipboard contract and optional system adapter.

mod contract;
#[cfg(feature = "clipboard")]
mod system;

pub use contract::{Clipboard, ClipboardError, ClipboardHandle};
#[cfg(feature = "clipboard")]
pub use system::{read_text, system_clipboard, write_text, SystemClipboard};
