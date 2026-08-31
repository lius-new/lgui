//! Platform-neutral tray menu contracts and application actions.

mod contract;
#[cfg(feature = "tray")]
mod model;

pub use contract::{TrayMenuEntry, TrayMenuItem, TrayService};
#[cfg(feature = "tray")]
pub use model::{TrayAction, TrayOptions};
