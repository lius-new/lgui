use std::{cell::RefCell, sync::Arc};

use crate::core::UiRect;

use super::{TextLayout, TextLayoutRequest, TextMeasureRequest, TextMetrics};

pub trait TextSystem: Send + Sync + 'static {
    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics>;

    fn layout(&self, _request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        None
    }
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

    fn layout(&self, request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        self.0.layout(request)
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

pub fn layout(request: &TextLayoutRequest<'_>) -> Option<TextLayout> {
    TEXT_SYSTEM.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|system| system.layout(request))
    })
}

pub fn measure_width(text: &str, rect: UiRect, font_height: f32, font_weight: i32) -> Option<f32> {
    let request = TextLayoutRequest::single_line(text, rect, font_height, font_weight);
    layout(&request).map(|layout| layout.width).or_else(|| {
        measure(&TextMeasureRequest {
            text,
            bounds: rect,
            font_height,
            font_weight,
        })
        .map(|metrics| metrics.width)
    })
}
