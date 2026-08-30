#![deny(unsafe_code)]

extern crate self as lgui;

pub use prelude::*;

pub mod application;
#[cfg(feature = "images")]
pub mod assets;
#[cfg(feature = "clipboard")]
pub mod clipboard;
pub mod command;
pub mod core;
#[cfg(feature = "open-url")]
pub mod desktop;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(feature = "dialogs")]
pub mod dialogs;
pub mod events;
pub mod frame;
pub mod host;
#[cfg(feature = "svg")]
pub mod icons;
pub mod platform;
pub mod prelude;
pub mod renderer;
pub mod resources;
#[cfg(feature = "router")]
pub mod router;
pub mod session;
#[cfg(feature = "store")]
pub mod store;
pub mod text;
#[cfg(feature = "theme")]
pub mod theme;
#[cfg(feature = "widgets")]
pub mod widgets;
