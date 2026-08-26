//! SVG icon data supplied to an Application.

use std::sync::Mutex;

pub use crate::platform::win32::{SvgIconRegistry, SvgIconSource};

pub(crate) struct IconRegistration(pub Mutex<Option<SvgIconRegistry>>);
