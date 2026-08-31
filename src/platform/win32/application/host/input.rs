fn dispatch_input(hwnd: HWND, input: InputEvent) {
    let (repaint, ime_update, frame_dispatcher) = STATE.with(|state| {
        if let Some(state) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
            let dispatcher = state.dispatcher.clone();
            let output = state.session.handle_input(input);
            let session = &mut state.session;
            let mut context_requested_frame = false;
            let output = dispatch_runtime_output(
                output,
                &state.context,
                &state.id,
                |action| session.handle_default_action(action),
                |context| context_requested_frame |= context.flags().needs_frame,
            );
            let ime_update =
                ime_composition_point(&output).map(|point| (state.logical_size, point));
            if context_requested_frame {
                session.invalidate_all();
                return (WindowRepaint::Full, ime_update, Some(dispatcher));
            }
            // Router and Store observables synchronously queue the component that owns their
            // outlet/content boundary. Consume that queue now so this input pass invalidates the
            // old boundary; Host diff adds the new boundary after the local component rerenders.
            let (mut repaint, frame_requested) = pending_session_repaint(session);
            if let Some(bounds) = output.dirty_bounds {
                session.invalidations_mut().invalidate_rect(bounds);
                repaint = repaint.union(WindowRepaint::Rect(bounds));
            }
            if output.animation_changed && repaint == WindowRepaint::None {
                session.invalidate_all();
                return (WindowRepaint::Full, ime_update, Some(dispatcher));
            }
            let frame_dispatcher =
                (frame_requested || output.animation_changed).then_some(dispatcher);
            return (repaint, ime_update, frame_dispatcher);
        }
        (WindowRepaint::None, None, None)
    });
    if let Some((logical_size, point)) = ime_update {
        update_ime_composition_window(hwnd, logical_size, point);
    }
    request_window_repaint(hwnd, repaint);
    if let Some(dispatcher) = frame_dispatcher {
        dispatcher.start_frame_driver();
    }
}

fn ime_composition_point(output: &RuntimeOutput) -> Option<Option<Point>> {
    output
        .events
        .iter()
        .filter_map(|event| match event {
            UiEvent::FocusChanged { current, .. } => Some(
                current
                    .as_ref()
                    .map(|hit| Point::new(hit.rect.left + 12.0, hit.rect.bottom + 4.0)),
            ),
            _ => None,
        })
        .last()
}

fn update_ime_composition_window(hwnd: HWND, logical_size: Size, point: Option<Point>) {
    unsafe {
        let _ = DestroyCaret();
    }
    let Some(point) = point else {
        return;
    };
    let physical_point = DpiContext::for_window(hwnd, logical_size)
        .scale
        .physical_point(point);
    unsafe {
        let context = ImmGetContext(hwnd);
        if context.is_invalid() {
            return;
        }
        let form = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT {
                x: physical_point.x,
                y: physical_point.y,
            },
            rcArea: Default::default(),
        };
        let _ = ImmSetCompositionWindow(context, &form);
        let _ = ImmReleaseContext(hwnd, context);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowRepaint {
    None,
    Rect(UiRect),
    Full,
}

impl WindowRepaint {
    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::Rect(left), Self::Rect(right)) => Self::Rect(left.union(right)),
            (Self::Rect(rect), Self::None) | (Self::None, Self::Rect(rect)) => Self::Rect(rect),
            (Self::None, Self::None) => Self::None,
        }
    }
}

fn pending_session_repaint(session: &mut UiSession) -> (WindowRepaint, bool) {
    let updates = session.apply_pending_updates();
    if updates.focus_changed {
        return (WindowRepaint::Full, updates.frame_requested);
    }
    if updates.dirty_ids.is_empty() {
        return (WindowRepaint::None, updates.frame_requested);
    }
    let repaint = session
        .tree()
        .paint_bounds(updates.dirty_ids)
        .map(WindowRepaint::Rect)
        .unwrap_or(WindowRepaint::Full);
    (repaint, updates.frame_requested)
}

fn apply_pending_window_updates(hwnd: HWND) -> (WindowRepaint, bool) {
    STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return (WindowRepaint::None, false);
        };
        pending_session_repaint(&mut window.session)
    })
}

fn request_window_repaint(hwnd: HWND, repaint: WindowRepaint) {
    if repaint != WindowRepaint::None {
        STATE.with(|state| {
            if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                window.render_retry_used = false;
            }
        });
    }
    match repaint {
        WindowRepaint::None => {}
        WindowRepaint::Full => unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        },
        WindowRepaint::Rect(logical) => {
            let physical = STATE.with(|state| {
                state.borrow().get(&(hwnd.0 as isize)).map(|window| {
                    DpiContext::for_window(hwnd, window.logical_size)
                        .scale
                        .physical_rect_outward(logical)
                })
            });
            if let Some(physical) = physical {
                let native = RECT {
                    left: physical.left,
                    top: physical.top,
                    right: physical.right,
                    bottom: physical.bottom,
                };
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), Some(&native), false);
                }
            }
        }
    }
}

fn logical_point(hwnd: HWND, point: PhysicalPoint) -> Point {
    STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|state| DpiContext::for_window(hwnd, state.logical_size).logical_point(point))
            .unwrap_or_else(|| Point::new(point.x as f32, point.y as f32))
    })
}

fn dispatch_mouse_button(hwnd: HWND, lparam: LPARAM, button: PointerButton, pressed: bool) {
    let pointer = PointerData::mouse(logical_point(hwnd, unpack_point(lparam)));
    if pressed {
        unsafe {
            let _ = SetCapture(hwnd);
        }
        dispatch_input(hwnd, InputEvent::PointerDown { pointer, button });
    } else {
        dispatch_input(hwnd, InputEvent::PointerUp { pointer, button });
        unsafe {
            let _ = ReleaseCapture();
        }
    }
}

fn track_mouse_leave(hwnd: HWND) {
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe {
        let _ = TrackMouseEvent(&mut tracking);
    }
}

fn current_logical_cursor(hwnd: HWND) -> Point {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
        let _ = ScreenToClient(hwnd, &mut point);
    }
    logical_point(hwnd, PhysicalPoint::new(point.x, point.y))
}

fn unpack_point(lparam: LPARAM) -> PhysicalPoint {
    PhysicalPoint::new(lparam.0 as i16 as i32, (lparam.0 >> 16) as i16 as i32)
}

fn logical_key(value: usize) -> LogicalKey {
    match value {
        0x08 => NamedKey::Backspace.into(),
        0x09 => NamedKey::Tab.into(),
        0x0D => NamedKey::Enter.into(),
        0x10 | 0xA0 | 0xA1 => NamedKey::Shift.into(),
        0x11 | 0xA2 | 0xA3 => NamedKey::Control.into(),
        0x12 | 0xA4 | 0xA5 => NamedKey::Alt.into(),
        0x14 => NamedKey::CapsLock.into(),
        0x1B => NamedKey::Escape.into(),
        0x21 => NamedKey::PageUp.into(),
        0x22 => NamedKey::PageDown.into(),
        0x23 => NamedKey::End.into(),
        0x24 => NamedKey::Home.into(),
        0x25 => NamedKey::ArrowLeft.into(),
        0x26 => NamedKey::ArrowUp.into(),
        0x27 => NamedKey::ArrowRight.into(),
        0x28 => NamedKey::ArrowDown.into(),
        0x2E => NamedKey::Delete.into(),
        0x5B | 0x5C => NamedKey::Meta.into(),
        0x90 => NamedKey::NumLock.into(),
        0x30..=0x39 | 0x41..=0x5A => {
            let mut character = char::from_u32(value as u32).unwrap_or_default();
            if !key_modifiers().shift() {
                character = character.to_ascii_lowercase();
            }
            LogicalKey::Character(character.to_string())
        }
        _ => NamedKey::Unidentified.into(),
    }
}

fn key_modifiers() -> KeyModifiers {
    let mut modifiers = KeyModifiers::empty();
    for (virtual_key, modifier) in [
        (VK_CONTROL.0 as i32, KeyModifiers::CONTROL),
        (VK_SHIFT.0 as i32, KeyModifiers::SHIFT),
        (0x12, KeyModifiers::ALT),
        (0x5B, KeyModifiers::META),
        (0x5C, KeyModifiers::META),
    ] {
        if unsafe { GetKeyState(virtual_key) } < 0 {
            modifiers.insert(modifier);
        }
    }
    for (virtual_key, modifier) in [
        (0x14, KeyModifiers::CAPS_LOCK),
        (0x90, KeyModifiers::NUM_LOCK),
    ] {
        if unsafe { GetKeyState(virtual_key) } & 1 != 0 {
            modifiers.insert(modifier);
        }
    }
    modifiers
}

fn keyboard_event(value: usize, lparam: LPARAM, state: KeyState) -> KeyboardEvent {
    let raw = lparam.0 as u32;
    KeyboardEvent {
        state,
        key: logical_key(value),
        code: physical_key(value),
        location: key_location(value, raw),
        modifiers: key_modifiers(),
        repeat: state == KeyState::Down && (raw & (1 << 30) != 0 || raw & 0xFFFF > 1),
        is_composing: false,
    }
}

fn physical_key(value: usize) -> PhysicalKey {
    match value {
        0x08 => PhysicalKey::Backspace,
        0x09 => PhysicalKey::Tab,
        0x0D => PhysicalKey::Enter,
        0x25 => PhysicalKey::ArrowLeft,
        0x26 => PhysicalKey::ArrowUp,
        0x27 => PhysicalKey::ArrowRight,
        0x28 => PhysicalKey::ArrowDown,
        0x41 => PhysicalKey::KeyA,
        0x43 => PhysicalKey::KeyC,
        0x56 => PhysicalKey::KeyV,
        0xA0 => PhysicalKey::ShiftLeft,
        0xA1 => PhysicalKey::ShiftRight,
        0xA2 => PhysicalKey::ControlLeft,
        0xA3 => PhysicalKey::ControlRight,
        0xA4 => PhysicalKey::AltLeft,
        0xA5 => PhysicalKey::AltRight,
        0x5B => PhysicalKey::MetaLeft,
        0x5C => PhysicalKey::MetaRight,
        _ => PhysicalKey::Unidentified,
    }
}

fn key_location(value: usize, raw_lparam: u32) -> KeyLocation {
    match value {
        0xA0 | 0xA2 | 0xA4 | 0x5B => KeyLocation::Left,
        0xA1 | 0xA3 | 0xA5 | 0x5C => KeyLocation::Right,
        0x60..=0x6F => KeyLocation::Numpad,
        0x0D if raw_lparam & (1 << 24) != 0 => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

fn take_window_char(hwnd: HWND, unit: u16) -> Option<String> {
    STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let Some(window) = windows.get_mut(&(hwnd.0 as isize)) else {
            return Some(String::from_utf16_lossy(&[unit]));
        };
        if suppress_committed_ime_char(&mut window.suppressed_ime_char_units, unit) {
            return None;
        }
        decode_utf16_char_unit(&mut window.pending_high_surrogate, unit)
    })
}

fn suppress_committed_ime_char(pending: &mut VecDeque<u16>, unit: u16) -> bool {
    if pending.front().copied() == Some(unit) {
        pending.pop_front();
        true
    } else {
        pending.clear();
        false
    }
}

fn decode_utf16_char_unit(pending_high_surrogate: &mut Option<u16>, unit: u16) -> Option<String> {
    if (0xD800..=0xDBFF).contains(&unit) {
        *pending_high_surrogate = Some(unit);
        return None;
    }
    let units = pending_high_surrogate
        .take()
        .map_or_else(|| vec![unit], |high| vec![high, unit]);
    Some(String::from_utf16_lossy(&units))
}

fn read_ime_string(
    hwnd: HWND,
    kind: windows::Win32::UI::Input::Ime::IME_COMPOSITION_STRING,
) -> Option<String> {
    unsafe {
        let context = ImmGetContext(hwnd);
        if context.is_invalid() {
            return None;
        }
        let byte_len = ImmGetCompositionStringW(context, kind, None, 0);
        if byte_len < 0 {
            let _ = ImmReleaseContext(hwnd, context);
            return None;
        }
        let mut units = vec![0_u16; byte_len as usize / size_of::<u16>()];
        if byte_len > 0 {
            let copied = ImmGetCompositionStringW(
                context,
                kind,
                Some(units.as_mut_ptr().cast()),
                byte_len as u32,
            );
            if copied < 0 {
                let _ = ImmReleaseContext(hwnd, context);
                return None;
            }
            units.truncate(copied as usize / size_of::<u16>());
        }
        let _ = ImmReleaseContext(hwnd, context);
        Some(String::from_utf16_lossy(&units))
    }
}

fn read_ime_cursor(hwnd: HWND) -> Option<usize> {
    unsafe {
        let context = ImmGetContext(hwnd);
        if context.is_invalid() {
            return None;
        }
        let cursor = ImmGetCompositionStringW(context, GCS_CURSORPOS, None, 0);
        let _ = ImmReleaseContext(hwnd, context);
        (cursor >= 0).then_some(cursor as usize)
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
