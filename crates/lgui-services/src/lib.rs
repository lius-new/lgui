//! Platform-neutral desktop service contracts and their optional system adapters.

#![deny(unsafe_code)]

mod application;

pub mod clipboard;
#[cfg(feature = "dialogs")]
pub mod dialogs;
pub mod notification;
#[cfg(feature = "open-url")]
pub mod open_url;
pub mod tray;

pub use application::{ServicesApplicationExt, ServicesContextExt};
#[cfg(feature = "tray")]
#[doc(hidden)]
pub use application::{TrayCommandHandler, TrayRegistration, dispatch_tray_action};
pub use clipboard::{Clipboard, ClipboardError, ClipboardHandle};
pub use notification::{Notification, NotificationError, NotificationHandle, NotificationService};
#[cfg(feature = "tray")]
pub use tray::{TrayAction, TrayOptions};
pub use tray::{TrayMenuEntry, TrayMenuItem, TrayService};
