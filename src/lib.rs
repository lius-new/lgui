#![deny(unsafe_code)]

extern crate self as lgui;

pub use prelude::*;

pub mod application;
#[cfg(feature = "images")]
pub mod assets;
#[cfg(feature = "clipboard")]
#[path = "services/clipboard.rs"]
pub mod clipboard;
pub mod command;
pub mod core;
#[cfg(feature = "open-url")]
#[path = "services/open_url.rs"]
pub mod desktop;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(feature = "dialogs")]
#[path = "services/dialogs.rs"]
pub mod dialogs;
pub mod events;
#[path = "runtime/frame/mod.rs"]
pub mod frame;
#[path = "runtime/host/mod.rs"]
pub mod host;
#[cfg(feature = "svg")]
#[path = "assets/icons.rs"]
pub mod icons;
pub mod platform;
pub mod prelude;
pub mod renderer;
pub mod resources;
#[cfg(feature = "router")]
pub mod router;
pub mod runtime;
pub mod services;
#[path = "runtime/session.rs"]
pub mod session;
#[cfg(feature = "store")]
pub mod store;
pub mod text;
#[cfg(feature = "theme")]
pub mod theme;
#[cfg(feature = "widgets")]
pub mod widgets;
