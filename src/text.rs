//! Platform-neutral text configuration and measurement.

use crate::core::UiRect;

pub(crate) struct FontFamilies(pub &'static [&'static str]);

pub fn measure_width(text: &str, rect: UiRect, font_height: i32, font_weight: i32) -> Option<i32> {
    crate::platform::win32::measure_text_width(text, rect, font_height, font_weight)
}
