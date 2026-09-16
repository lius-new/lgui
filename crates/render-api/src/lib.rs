#![deny(unsafe_code)]

//! Backend-neutral rendering contracts for LGUI.

mod contract;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;

pub use contract::*;
