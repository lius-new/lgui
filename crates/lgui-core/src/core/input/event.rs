use std::{collections::HashMap, ops::Range, path::PathBuf};

use super::{
    normalized_f32_bits, AnimProperty, AnimationRegistry, EventPolicy, HitResult, HostTree, Point,
    UiId,
};

pub use keyboard_types::{
    Code as PhysicalKey, Key as LogicalKey, KeyState, KeyboardEvent, Location as KeyLocation,
    Modifiers as KeyModifiers, NamedKey,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Backend-neutral mouse cursor icon.
///
/// Nodes can request a specific cursor (e.g. a text I-beam for editable
/// regions). The platform backend resolves the cursor for the frontmost node
/// under the pointer and maps it to a native cursor icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorIcon {
    #[default]
    Default,
    Text,
    Pointer,
    Move,
    NotAllowed,
    Crosshair,
    Wait,
    ResizeHorizontal,
    ResizeVertical,
    ResizeDiagonalTopLeftBottomRight,
    ResizeDiagonalTopRightBottomLeft,
    ResizeColumn,
    ResizeRow,
    ScrollAll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PointerId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PointerKind {
    #[default]
    Mouse,
    Touch,
    Pen,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerData {
    pub id: PointerId,
    pub kind: PointerKind,
    pub point: Point,
    pub pressure: Option<u16>,
    pub primary: bool,
}

impl PointerData {
    pub const fn mouse(point: Point) -> Self {
        Self {
            id: PointerId(0),
            kind: PointerKind::Mouse,
            point,
            pressure: None,
            primary: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelUnit {
    Lines,
    Pixels,
}

#[derive(Clone, Copy, Debug)]
pub struct WheelDelta {
    pub x: f32,
    pub y: f32,
    pub unit: WheelUnit,
    pub phase: TouchPhase,
}

impl PartialEq for WheelDelta {
    fn eq(&self, other: &Self) -> bool {
        normalized_f32_bits(self.x) == normalized_f32_bits(other.x)
            && normalized_f32_bits(self.y) == normalized_f32_bits(other.y)
            && self.unit == other.unit
            && self.phase == other.phase
    }
}

impl Eq for WheelDelta {}

impl WheelDelta {
    pub const fn lines(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            unit: WheelUnit::Lines,
            phase: TouchPhase::Moved,
        }
    }

    pub const fn pixels(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            unit: WheelUnit::Pixels,
            phase: TouchPhase::Moved,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    PointerMove(PointerData),
    PointerDown {
        pointer: PointerData,
        button: PointerButton,
    },
    PointerUp {
        pointer: PointerData,
        button: PointerButton,
    },
    PointerEnter(PointerData),
    Wheel {
        point: Point,
        delta: WheelDelta,
    },
    TextInput(String),
    Ime(ImeEvent),
    Keyboard(KeyboardEvent),
    PointerLeave(PointerData),
    Touch {
        pointer: PointerData,
        phase: TouchPhase,
    },
    Platform(PlatformEvent),
    Semantic(SemanticInput),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticInput {
    Click(UiId),
    Focus(UiId),
    Blur(UiId),
    SetValue {
        target: UiId,
        value: String,
    },
    Action {
        target: UiId,
        action: super::SemanticAction,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformTheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowStateEvent {
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub fullscreen: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    Focused(bool),
    FileHovered(PathBuf),
    FileDropped(PathBuf),
    FileHoverCancelled,
    ScaleFactorChanged(f64),
    ThemeChanged(PlatformTheme),
    WindowStateChanged(WindowStateEvent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    Enabled,
    Preedit {
        text: String,
        cursor: Option<Range<usize>>,
    },
    Commit(String),
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    HoverChanged {
        previous: Option<UiId>,
        current: Option<HitResult>,
    },
    PressedChanged {
        previous: Option<UiId>,
        current: Option<HitResult>,
    },
    Clicked(HitResult),
    Wheel {
        hit: HitResult,
        delta: WheelDelta,
    },
    TextInput {
        target: UiId,
        text: String,
    },
    ImeStarted {
        target: UiId,
    },
    ImeUpdated {
        target: UiId,
        text: String,
        cursor: Option<Range<usize>>,
    },
    ImeEnded {
        target: UiId,
    },
    Keyboard {
        target: UiId,
        event: KeyboardEvent,
    },
    PointerPressed {
        hit: HitResult,
        pointer: PointerData,
    },
    PointerMoved {
        hit: HitResult,
        pointer: PointerData,
    },
    PointerDragged {
        hit: HitResult,
        pointer: PointerData,
    },
    PointerReleased {
        hit: HitResult,
        pointer: PointerData,
    },
    FocusChanged {
        previous: Option<UiId>,
        current: Option<HitResult>,
    },
    PointerLeft {
        previous: Option<UiId>,
    },
    SemanticValue {
        target: UiId,
        value: String,
    },
    SemanticAction {
        target: UiId,
        action: super::SemanticAction,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiInteractionState {
    pub hovered: Option<UiId>,
    pub pressed: Option<UiId>,
    pub focused: Option<UiId>,
}

impl UiInteractionState {
    pub fn flags_for(&self, id: &UiId) -> InteractionFlags {
        InteractionFlags {
            hovered: self.hovered.as_ref() == Some(id),
            pressed: self.pressed.as_ref() == Some(id),
            focused: self.focused.as_ref() == Some(id),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InteractionFlags {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

mod animation;
mod dispatcher;

pub use animation::apply_events_to_animations;
pub use dispatcher::UiEventDispatcher;

#[cfg(test)]
#[path = "event_test.rs"]
mod tests;
