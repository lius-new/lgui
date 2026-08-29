use std::{ops::Range, sync::Arc};

use super::{ActionId, KeyboardEvent, PointerData, UiAction, UiEventContext, UiId, WheelDelta};

pub type UiEventHandler = Arc<dyn Fn(&mut UiEventContext) + Send + Sync>;
pub type UiInputEventHandler = Arc<dyn Fn(&mut UiEventContext, &UiEventPayload) + Send + Sync>;
pub type UiActionHandler = Arc<dyn Fn(&mut UiEventContext, &UiAction) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiEventKind {
    Click,
    PointerDown,
    PointerMove,
    PointerUp,
    Wheel,
    KeyDown,
    KeyUp,
    Input,
    CompositionStart,
    CompositionUpdate,
    CompositionEnd,
    Focus,
    Blur,
    Change,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEventPayload {
    Click,
    PointerDown {
        pointer: PointerData,
    },
    PointerMove {
        pointer: PointerData,
    },
    PointerUp {
        pointer: PointerData,
    },
    Wheel {
        delta: WheelDelta,
    },
    Keyboard {
        event: KeyboardEvent,
    },
    Input {
        text: String,
    },
    CompositionStart,
    CompositionUpdate {
        text: String,
        cursor: Option<Range<usize>>,
    },
    CompositionEnd,
    Focus,
    Blur,
    Change {
        value: Option<String>,
    },
}

impl UiEventPayload {
    pub const fn kind(&self) -> UiEventKind {
        match self {
            Self::Click => UiEventKind::Click,
            Self::PointerDown { .. } => UiEventKind::PointerDown,
            Self::PointerMove { .. } => UiEventKind::PointerMove,
            Self::PointerUp { .. } => UiEventKind::PointerUp,
            Self::Wheel { .. } => UiEventKind::Wheel,
            Self::Keyboard {
                event:
                    KeyboardEvent {
                        state: super::KeyState::Down,
                        ..
                    },
            } => UiEventKind::KeyDown,
            Self::Keyboard { .. } => UiEventKind::KeyUp,
            Self::Input { .. } => UiEventKind::Input,
            Self::CompositionStart => UiEventKind::CompositionStart,
            Self::CompositionUpdate { .. } => UiEventKind::CompositionUpdate,
            Self::CompositionEnd => UiEventKind::CompositionEnd,
            Self::Focus => UiEventKind::Focus,
            Self::Blur => UiEventKind::Blur,
            Self::Change { .. } => UiEventKind::Change,
        }
    }
}

#[derive(Clone)]
pub struct UiInputEventBinding {
    pub kind: UiEventKind,
    pub capture: bool,
    pub handler: UiInputEventHandler,
}

#[derive(Clone)]
pub struct UiHandlerEvent {
    pub target: UiId,
    pub payload: UiEventPayload,
    pub capture_handlers: Vec<UiInputEventHandler>,
    pub bubble_handlers: Vec<UiInputEventHandler>,
}

impl std::fmt::Debug for UiHandlerEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UiHandlerEvent")
            .field("target", &self.target)
            .field("payload", &self.payload)
            .field("capture_handlers", &self.capture_handlers.len())
            .field("bubble_handlers", &self.bubble_handlers.len())
            .finish()
    }
}

#[derive(Clone)]
pub struct UiActionEvent {
    pub target: UiId,
    pub action: UiAction,
    pub handler: UiActionHandler,
}

impl UiActionEvent {
    pub fn dispatch(&self, context: &mut UiEventContext) {
        (self.handler)(context, &self.action);
    }
}

impl std::fmt::Debug for UiActionEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UiActionEvent")
            .field("target", &self.target)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

pub struct UiActionBinding {
    pub id: ActionId,
    pub handler: UiActionHandler,
}

impl Clone for UiActionBinding {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

pub trait IntoUiHandler {
    fn into_handler(self) -> UiEventHandler;
}

impl<F> IntoUiHandler for F
where
    F: Fn(&mut UiEventContext) + Send + Sync + 'static,
{
    fn into_handler(self) -> UiEventHandler {
        Arc::new(self)
    }
}

impl IntoUiHandler for UiEventHandler {
    fn into_handler(self) -> UiEventHandler {
        self
    }
}
