#![deny(unsafe_code)]

//! Backend-neutral rendering contracts for LGUI.

mod contract;
mod preference;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;

pub use contract::*;
pub use preference::GraphicsPreference;
