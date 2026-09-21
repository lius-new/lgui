use super::*;
use crate::core::PointerButton;

pub trait IntoClickHandler<Arguments> {
    fn into_click_handler(self) -> UiEventHandler;
}

pub enum NoClickArguments {}
pub enum ClickEventArgument {}

impl<F> IntoClickHandler<NoClickArguments> for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn into_click_handler(self) -> UiEventHandler {
        Arc::new(move |_| self())
    }
}

impl<F> IntoClickHandler<ClickEventArgument> for F
where
    F: Fn(&mut UiEventContext) + Send + Sync + 'static,
{
    fn into_click_handler(self) -> UiEventHandler {
        Arc::new(self)
    }
}

impl Element {
    pub fn on_click_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_handler = Some(handler);
        self
    }

    pub fn on_click_capture_handler(mut self, handler: UiEventHandler) -> Self {
        self.click_capture_handler = Some(handler);
        self
    }

    pub fn event_policy(mut self, policy: EventPolicy) -> Self {
        self.event_policy = Some(policy);
        self
    }

    pub fn on_click<F, Arguments>(self, handler: F) -> Self
    where
        F: IntoClickHandler<Arguments>,
    {
        self.on_click_handler(handler.into_click_handler())
    }

    pub fn on_click_event<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_click_handler(Arc::new(handler))
    }

    pub fn on_click_async<F, Fut>(self, handler: F) -> Self
    where
        F: Fn(UiAsyncContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_click_handler(async_handler(handler))
    }

    pub fn on_click_capture<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_click_capture_handler(Arc::new(handler))
    }

    pub fn on_event_handler(
        mut self,
        kind: UiEventKind,
        capture: bool,
        handler: UiInputEventHandler,
    ) -> Self {
        self.input_event_handlers.push(UiInputEventBinding {
            kind,
            capture,
            handler,
        });
        self
    }

    pub fn on_event<F>(self, kind: UiEventKind, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync + 'static,
    {
        self.on_event_handler(kind, false, Arc::new(handler))
    }

    pub fn on_event_capture<F>(self, kind: UiEventKind, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync + 'static,
    {
        self.on_event_handler(kind, true, Arc::new(handler))
    }

    pub fn on_pointer_down<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, PointerData) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerDown, move |cx, payload| {
            if let UiEventPayload::PointerDown { pointer, .. } = payload {
                handler(cx, *pointer);
            }
        })
    }

    pub fn on_pointer_down_with_button<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, PointerData, PointerButton) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerDown, move |cx, payload| {
            if let UiEventPayload::PointerDown { pointer, button } = payload {
                handler(cx, *pointer, *button);
            }
        })
    }

    pub fn on_pointer_move<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, PointerData) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerMove, move |cx, payload| {
            if let UiEventPayload::PointerMove { pointer } = payload {
                handler(cx, *pointer);
            }
        })
    }

    pub fn on_pointer_up<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, PointerData) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::PointerUp, move |cx, payload| {
            if let UiEventPayload::PointerUp { pointer } = payload {
                handler(cx, *pointer);
            }
        })
    }

    pub fn on_wheel<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, WheelDelta) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Wheel, move |cx, payload| {
            if let UiEventPayload::Wheel { delta } = payload {
                handler(cx, *delta);
            }
        })
    }

    pub fn on_key_down<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &KeyboardEvent) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::KeyDown, move |cx, payload| {
            if let UiEventPayload::Keyboard { event } = payload {
                handler(cx, event);
            }
        })
    }

    pub fn on_key_up<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &KeyboardEvent) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::KeyUp, move |cx, payload| {
            if let UiEventPayload::Keyboard { event } = payload {
                handler(cx, event);
            }
        })
    }

    pub fn on_input<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &str) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Input, move |cx, payload| {
            if let UiEventPayload::Input { text } = payload {
                handler(cx, text);
            }
        })
    }

    pub fn on_composition_start<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::CompositionStart, move |cx, _| handler(cx))
    }

    pub fn on_composition_update<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, &str, Option<std::ops::Range<usize>>) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::CompositionUpdate, move |cx, payload| {
            if let UiEventPayload::CompositionUpdate { text, cursor } = payload {
                handler(cx, text, cursor.clone());
            }
        })
    }

    pub fn on_composition_end<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::CompositionEnd, move |cx, _| handler(cx))
    }

    pub fn on_focus<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Focus, move |cx, _| handler(cx))
    }

    pub fn on_blur<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Blur, move |cx, _| handler(cx))
    }

    pub fn on_change<F>(self, handler: F) -> Self
    where
        F: Fn(&mut UiEventContext, Option<&str>) + Send + Sync + 'static,
    {
        self.on_event(UiEventKind::Change, move |cx, payload| {
            if let UiEventPayload::Change { value } = payload {
                handler(cx, value.as_deref());
            }
        })
    }
}
