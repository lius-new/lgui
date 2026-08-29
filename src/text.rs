//! Platform-neutral text configuration and measurement.

use std::{cell::RefCell, sync::Arc};

use crate::core::UiRect;

#[derive(Clone, Copy)]
pub(crate) struct FontFamilies(pub &'static [&'static str]);

thread_local! {
    static FONT_FAMILIES: RefCell<Vec<&'static [&'static str]>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct FontFamiliesGuard;

impl Drop for FontFamiliesGuard {
    fn drop(&mut self) {
        FONT_FAMILIES.with(|current| {
            current.borrow_mut().pop();
        });
    }
}

pub(crate) fn install_font_families(families: &'static [&'static str]) -> FontFamiliesGuard {
    FONT_FAMILIES.with(|current| current.borrow_mut().push(families));
    FontFamiliesGuard
}

pub(crate) fn font_families() -> &'static [&'static str] {
    FONT_FAMILIES.with(|current| current.borrow().last().copied().unwrap_or(&["Segoe UI"]))
}

#[derive(Clone, Copy, Debug)]
pub struct TextMeasureRequest<'a> {
    pub text: &'a str,
    pub bounds: UiRect,
    pub font_height: f32,
    pub font_weight: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
}

pub trait TextSystem: Send + Sync + 'static {
    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics>;
}

#[derive(Clone)]
pub struct TextSystemHandle(Arc<dyn TextSystem>);

impl TextSystemHandle {
    pub fn new(system: impl TextSystem) -> Self {
        Self(Arc::new(system))
    }

    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
        self.0.measure(request)
    }
}

thread_local! {
    static TEXT_SYSTEM: RefCell<Option<TextSystemHandle>> = const { RefCell::new(None) };
}

pub(crate) struct TextSystemGuard {
    previous: Option<TextSystemHandle>,
}

impl Drop for TextSystemGuard {
    fn drop(&mut self) {
        TEXT_SYSTEM.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

pub(crate) fn install_text_system(system: TextSystemHandle) -> TextSystemGuard {
    let previous = TEXT_SYSTEM.with(|current| current.borrow_mut().replace(system));
    TextSystemGuard { previous }
}

pub fn measure(request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
    TEXT_SYSTEM.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|system| system.measure(request))
    })
}

pub fn measure_width(text: &str, rect: UiRect, font_height: f32, font_weight: i32) -> Option<f32> {
    measure(&TextMeasureRequest {
        text,
        bounds: rect,
        font_height,
        font_weight,
    })
    .map(|metrics| metrics.width)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedTextSystem;

    impl TextSystem for FixedTextSystem {
        fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
            Some(TextMetrics {
                width: request.text.len() as f32 * 3.0,
            })
        }
    }

    #[test]
    fn text_system_capability_is_scoped_and_restored() {
        let bounds = UiRect::new(0.0, 0.0, 100.0, 20.0);
        assert_eq!(measure_width("abc", bounds, -14.0, 400), None);
        {
            let _guard = install_text_system(TextSystemHandle::new(FixedTextSystem));
            assert_eq!(measure_width("abc", bounds, -14.0, 400), Some(9.0));
        }
        assert_eq!(measure_width("abc", bounds, -14.0, 400), None);
    }
}
