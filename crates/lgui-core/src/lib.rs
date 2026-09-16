#![deny(unsafe_code)]

extern crate self as lgui;

pub use prelude::*;

pub mod application;
#[cfg(feature = "images")]
pub mod assets;
#[doc(hidden)]
pub mod backend;
#[cfg(feature = "clipboard")]
pub use services::clipboard;
pub mod command;
pub mod core;
#[cfg(feature = "open-url")]
pub use services::open_url as desktop;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(feature = "dialogs")]
pub use services::dialogs;
pub mod events;
#[path = "runtime/frame/mod.rs"]
pub mod frame;
#[path = "runtime/host/mod.rs"]
pub mod host;
#[cfg(feature = "svg")]
#[path = "assets/icons.rs"]
pub mod icons;
pub mod memory;
pub mod platform;
pub mod prelude;
pub mod renderer;
pub mod resources;
pub mod runtime;
pub mod services;
#[path = "runtime/session.rs"]
pub mod session;
pub mod text;
pub mod window;
