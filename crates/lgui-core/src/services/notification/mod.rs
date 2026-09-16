//! Platform-neutral notification model, service contract, and handle.

mod contract;

pub use contract::{Notification, NotificationError, NotificationHandle, NotificationService};
