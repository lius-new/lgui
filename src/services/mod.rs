//! Platform-neutral desktop service contracts and their optional system adapters.

pub use crate::platform::{
    Clipboard, ClipboardError, ClipboardHandle, Notification, NotificationError,
    NotificationHandle, NotificationService, TrayMenuEntry, TrayMenuItem, TrayService,
};

#[cfg(feature = "clipboard")]
pub use crate::clipboard;
#[cfg(feature = "open-url")]
pub use crate::desktop as open_url;
#[cfg(feature = "dialogs")]
pub use crate::dialogs;
