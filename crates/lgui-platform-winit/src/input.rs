use super::*;

pub(super) fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        MouseButton::Other(value) => PointerButton::Other(value),
    }
}

pub(super) fn resize_direction(
    point: PhysicalPoint,
    size: WinitPhysicalSize<u32>,
    border: i32,
) -> Option<ResizeDirection> {
    let left = point.x >= 0 && point.x < border;
    let right = point.x < size.width as i32 && point.x >= size.width as i32 - border;
    let top = point.y >= 0 && point.y < border;
    let bottom = point.y < size.height as i32 && point.y >= size.height as i32 - border;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (_, true, true, _) => Some(ResizeDirection::NorthEast),
        (true, _, _, true) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, _, _) => Some(ResizeDirection::West),
        (_, true, _, _) => Some(ResizeDirection::East),
        (_, _, true, _) => Some(ResizeDirection::North),
        (_, _, _, true) => Some(ResizeDirection::South),
        _ => None,
    }
}

pub(super) fn edge_resize_cursor(direction: ResizeDirection) -> WinitCursorIcon {
    match direction {
        ResizeDirection::North | ResizeDirection::South => WinitCursorIcon::NsResize,
        ResizeDirection::East | ResizeDirection::West => WinitCursorIcon::EwResize,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => WinitCursorIcon::NwseResize,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => WinitCursorIcon::NeswResize,
    }
}

pub(super) fn mapped_cursor(cursor: CursorIcon) -> WinitCursorIcon {
    match cursor {
        CursorIcon::Default => WinitCursorIcon::Default,
        CursorIcon::Text => WinitCursorIcon::Text,
        CursorIcon::Pointer => WinitCursorIcon::Pointer,
        CursorIcon::Move => WinitCursorIcon::Move,
        CursorIcon::NotAllowed => WinitCursorIcon::NotAllowed,
        CursorIcon::Crosshair => WinitCursorIcon::Crosshair,
        CursorIcon::Wait => WinitCursorIcon::Wait,
        CursorIcon::ResizeHorizontal => WinitCursorIcon::EwResize,
        CursorIcon::ResizeVertical => WinitCursorIcon::NsResize,
        CursorIcon::ResizeDiagonalTopLeftBottomRight => WinitCursorIcon::NwseResize,
        CursorIcon::ResizeDiagonalTopRightBottomLeft => WinitCursorIcon::NeswResize,
        CursorIcon::ResizeColumn => WinitCursorIcon::ColResize,
        CursorIcon::ResizeRow => WinitCursorIcon::RowResize,
        CursorIcon::ScrollAll => WinitCursorIcon::AllScroll,
    }
}

pub(super) fn touch_phase(phase: WinitTouchPhase) -> TouchPhase {
    match phase {
        WinitTouchPhase::Started => TouchPhase::Started,
        WinitTouchPhase::Moved => TouchPhase::Moved,
        WinitTouchPhase::Ended => TouchPhase::Ended,
        WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
    }
}

pub(super) fn keyboard_event(
    event: winit::event::KeyEvent,
    modifiers: ModifiersState,
) -> KeyboardEvent {
    let key = match event.logical_key {
        WinitKey::Character(value) => LogicalKey::Character(value.to_string()),
        WinitKey::Named(value) => format!("{value:?}")
            .parse()
            .map(LogicalKey::Named)
            .unwrap_or_else(|_| LogicalKey::Character(format!("{value:?}"))),
        WinitKey::Unidentified(_) => LogicalKey::Character(String::new()),
        WinitKey::Dead(value) => {
            LogicalKey::Character(value.map_or_else(String::new, |value| value.to_string()))
        }
    };
    let code = match event.physical_key {
        WinitPhysicalKey::Code(value) => format!("{value:?}").parse().unwrap_or_default(),
        WinitPhysicalKey::Unidentified(_) => PhysicalKey::default(),
    };
    let location = match event.location {
        WinitKeyLocation::Standard => KeyLocation::Standard,
        WinitKeyLocation::Left => KeyLocation::Left,
        WinitKeyLocation::Right => KeyLocation::Right,
        WinitKeyLocation::Numpad => KeyLocation::Numpad,
    };
    let mut translated = KeyModifiers::empty();
    if modifiers.alt_key() {
        translated |= KeyModifiers::ALT;
    }
    if modifiers.control_key() {
        translated |= KeyModifiers::CONTROL;
    }
    if modifiers.shift_key() {
        translated |= KeyModifiers::SHIFT;
    }
    if modifiers.super_key() {
        translated |= KeyModifiers::META;
    }
    KeyboardEvent {
        state: match event.state {
            ElementState::Pressed => KeyState::Down,
            ElementState::Released => KeyState::Up,
        },
        key,
        code,
        location,
        modifiers: translated,
        repeat: event.repeat,
        is_composing: false,
    }
}
