extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCALCSIZE => {
            let custom_frame = STATE.with(|state| {
                state
                    .borrow()
                    .get(&(hwnd.0 as isize))
                    .is_some_and(|state| !state.native_titlebar)
            });
            if custom_frame {
                // The full window rectangle belongs to the client when native decorations are
                // disabled. Without handling this message Windows can retain a non-client caption
                // even after WS_CAPTION has been removed.
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_NCHITTEST => custom_frame_hit_test(hwnd, lparam)
            .unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }),
        WM_LGUI_DISPATCH => {
            drain_dispatcher(hwnd);
            LRESULT(0)
        }
        WM_LGUI_FRAME_TICK => {
            handle_frame_tick(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            let resize_dispatcher = STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let window = windows.get_mut(&(hwnd.0 as isize))?;
                if window.interaction_mode != WindowInteractionMode::Sizing {
                    return None;
                }
                window.resize_frame_throttle.request();
                Some(window.dispatcher.clone())
            });
            if wparam.0 as u32 == SIZE_MINIMIZED {
                hide_owned_windows(hwnd);
            } else if resize_dispatcher.is_none() {
                restore_owned_windows(hwnd);
            }
            if let Some(dispatcher) = resize_dispatcher {
                dispatcher.start_frame_driver();
            } else {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_MOVE => {
            reposition_owned_windows(hwnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ENTERSIZEMOVE => {
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                    window.interaction_mode = WindowInteractionMode::MoveResize;
                    window.resize_frame_throttle.reset();
                }
            });
            hide_owned_windows(hwnd);
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_MOVING => {
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                    window.interaction_mode = WindowInteractionMode::Moving;
                }
            });
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_SIZING => {
            let dispatcher = STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let window = windows.get_mut(&(hwnd.0 as isize))?;
                if window.interaction_mode != WindowInteractionMode::Sizing {
                    window.interaction_mode = WindowInteractionMode::Sizing;
                    window.resize_frame_throttle.begin();
                }
                Some(window.dispatcher.clone())
            });
            if let Some(dispatcher) = dispatcher {
                dispatcher.start_frame_driver();
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            let exit = STATE.with(|state| {
                let mut windows = state.borrow_mut();
                let window = windows.get_mut(&(hwnd.0 as isize))?;
                let was_sizing = window.interaction_mode == WindowInteractionMode::Sizing;
                window.interaction_mode = WindowInteractionMode::Idle;
                window.resize_frame_throttle.reset();
                Some((
                    was_sizing,
                    window
                        .can_advance_animations()
                        .then(|| window.dispatcher.clone()),
                ))
            });
            restore_owned_windows(hwnd);
            if let Some((was_sizing, dispatcher)) = exit {
                if was_sizing {
                    request_window_repaint(hwnd, WindowRepaint::Full);
                }
                if let Some(dispatcher) = dispatcher {
                    dispatcher.start_frame_driver();
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                let hide = STATE.with(|state| {
                    state
                        .borrow()
                        .get(&(hwnd.0 as isize))
                        .is_some_and(|state| state.hide_on_deactivate)
                });
                if hide {
                    hide_window(hwnd);
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_GETMINMAXINFO => {
            STATE.with(|state| {
                let state = state.borrow();
                let Some(state) = state.get(&(hwnd.0 as isize)) else {
                    return;
                };
                let scale = DpiContext::for_window(hwnd, state.logical_size).scale;
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                if let Some(minimum) = state.minimum_size {
                    let physical = scale.physical_size(minimum);
                    info.ptMinTrackSize.x = physical.width;
                    info.ptMinTrackSize.y = physical.height;
                }
                if let Some(maximum) = state.maximum_size {
                    let physical = scale.physical_size(maximum);
                    info.ptMaxTrackSize.x = physical.width;
                    info.ptMaxTrackSize.y = physical.height;
                }
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let suggested = unsafe { *(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            dispatch_mouse_button(hwnd, lparam, PointerButton::Left, true);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            dispatch_mouse_button(hwnd, lparam, PointerButton::Left, false);
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            dispatch_mouse_button(hwnd, lparam, PointerButton::Right, true);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            dispatch_mouse_button(hwnd, lparam, PointerButton::Right, false);
            LRESULT(0)
        }
        WM_MBUTTONDOWN => {
            dispatch_mouse_button(hwnd, lparam, PointerButton::Middle, true);
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            dispatch_mouse_button(hwnd, lparam, PointerButton::Middle, false);
            LRESULT(0)
        }
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let button = if (wparam.0 >> 16) as u16 == 1 {
                PointerButton::Back
            } else {
                PointerButton::Forward
            };
            dispatch_mouse_button(hwnd, lparam, button, message == WM_XBUTTONDOWN);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let pointer = PointerData::mouse(logical_point(hwnd, unpack_point(lparam)));
            let entered = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let Some(window) = state.get_mut(&(hwnd.0 as isize)) else {
                    return false;
                };
                if window.pointer_inside {
                    false
                } else {
                    window.pointer_inside = true;
                    true
                }
            });
            track_mouse_leave(hwnd);
            if entered {
                dispatch_input(hwnd, InputEvent::PointerEnter(pointer));
            }
            dispatch_input(hwnd, InputEvent::PointerMove(pointer));
            LRESULT(0)
        }
        WM_MOUSE_LEAVE => {
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                    window.pointer_inside = false;
                }
            });
            dispatch_input(
                hwnd,
                InputEvent::PointerLeave(PointerData::mouse(current_logical_cursor(hwnd))),
            );
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let mut point = POINT {
                x: lparam.0 as i16 as i32,
                y: (lparam.0 >> 16) as i16 as i32,
            };
            unsafe {
                let _ = ScreenToClient(hwnd, &mut point);
            }
            dispatch_input(
                hwnd,
                InputEvent::Wheel {
                    point: logical_point(hwnd, PhysicalPoint::new(point.x, point.y)),
                    delta: WheelDelta::lines(0.0, (wparam.0 >> 16) as i16 as f32 / 120.0),
                },
            );
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            let mut point = POINT {
                x: lparam.0 as i16 as i32,
                y: (lparam.0 >> 16) as i16 as i32,
            };
            unsafe {
                let _ = ScreenToClient(hwnd, &mut point);
            }
            dispatch_input(
                hwnd,
                InputEvent::Wheel {
                    point: logical_point(hwnd, PhysicalPoint::new(point.x, point.y)),
                    delta: WheelDelta::lines((wparam.0 >> 16) as i16 as f32 / 120.0, 0.0),
                },
            );
            LRESULT(0)
        }
        WM_CHAR => {
            let unit = wparam.0 as u16;
            if let Some(text) =
                take_window_char(hwnd, unit).filter(|text| !text.chars().all(char::is_control))
            {
                dispatch_input(hwnd, InputEvent::TextInput(text));
            }
            LRESULT(0)
        }
        WM_IME_STARTCOMPOSITION => {
            dispatch_input(hwnd, InputEvent::Ime(ImeEvent::Enabled));
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_IME_COMPOSITION => {
            let flags = lparam.0 as u32;
            if flags & GCS_RESULTSTR.0 != 0 {
                if let Some(text) = read_ime_string(hwnd, GCS_RESULTSTR) {
                    STATE.with(|state| {
                        if let Some(window) = state.borrow_mut().get_mut(&(hwnd.0 as isize)) {
                            window.suppressed_ime_char_units.extend(text.encode_utf16());
                            window.pending_high_surrogate = None;
                        }
                    });
                    dispatch_input(hwnd, InputEvent::Ime(ImeEvent::Commit(text)));
                }
            } else if flags & GCS_COMPSTR.0 != 0 {
                let text = read_ime_string(hwnd, GCS_COMPSTR).unwrap_or_default();
                let cursor = read_ime_cursor(hwnd).map(|cursor| cursor..cursor);
                dispatch_input(hwnd, InputEvent::Ime(ImeEvent::Preedit { text, cursor }));
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_IME_ENDCOMPOSITION => {
            dispatch_input(hwnd, InputEvent::Ime(ImeEvent::Disabled));
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            dispatch_input(
                hwnd,
                InputEvent::Keyboard(keyboard_event(wparam.0, lparam, KeyState::Down)),
            );
            if message == WM_SYSKEYDOWN {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            } else {
                LRESULT(0)
            }
        }
        WM_KEYUP | WM_SYSKEYUP => {
            dispatch_input(
                hwnd,
                InputEvent::Keyboard(keyboard_event(wparam.0, lparam, KeyState::Up)),
            );
            if message == WM_SYSKEYUP {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            } else {
                LRESULT(0)
            }
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            request_window_close(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let (empty, dispatcher, next_window) = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let dispatcher = state
                    .remove(&(hwnd.0 as isize))
                    .map(|window| window.dispatcher);
                let next_window = state.keys().next().copied();
                (state.is_empty(), dispatcher, next_window)
            });
            if let Some(dispatcher) = dispatcher {
                dispatcher.detach(hwnd);
                if let Some(raw) = next_window {
                    dispatcher.attach(HWND(raw as _));
                } else {
                    dispatcher.stop_frame_driver();
                }
            }
            if empty {
                unsafe {
                    PostQuitMessage(0);
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn request_window_close(hwnd: HWND) {
    let request = STATE.with(|state| {
        state.borrow().get(&(hwnd.0 as isize)).map(|state| {
            (
                state.close_policy,
                state.close_handler,
                state.context.clone(),
                state.id.clone(),
            )
        })
    });
    let Some((policy, handler, context, id)) = request else {
        return;
    };
    match policy {
        ClosePolicy::Exit => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        ClosePolicy::Hide => {
            hide_window(hwnd);
        }
        ClosePolicy::Notify => {
            if let Some(handler) = handler {
                let mut event = crate::core::UiEventContext::new(context, id);
                handler(&mut event);
                if event.flags().needs_frame {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
        }
    }
}

fn drain_dispatcher(hwnd: HWND) {
    let dispatcher = STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|window| window.dispatcher.clone())
    });
    let Some(dispatcher) = dispatcher else {
        return;
    };
    let result = dispatcher.drain();
    let mut start_frame_driver = result.frame_requested;
    let windows = STATE.with(|state| state.borrow().keys().copied().collect::<Vec<_>>());
    for raw in windows {
        let target = HWND(raw as _);
        let (repaint, frame_requested) = apply_pending_window_updates(target);
        start_frame_driver |= frame_requested;
        request_window_repaint(target, repaint);
    }
    if start_frame_driver {
        dispatcher.start_frame_driver();
    }
}

fn handle_frame_tick(hwnd: HWND) {
    let dispatcher = STATE.with(|state| {
        state
            .borrow()
            .get(&(hwnd.0 as isize))
            .map(|window| window.dispatcher.clone())
    });
    let Some(dispatcher) = dispatcher else {
        return;
    };
    let elapsed_ms = dispatcher.frame_elapsed_ms();
    let (repaints, should_continue, frame_interval_ms) = STATE.with(|state| {
        let mut windows = state.borrow_mut();
        let mut repaints = Vec::new();
        let mut should_continue = false;
        let mut frame_interval_ms = None::<u64>;
        for (raw, window) in windows.iter_mut() {
            if window.rendering_suspended
                || !window.visibility.desired_visible
                || window.visibility.hidden_for_owner
            {
                continue;
            }
            match window.interaction_mode {
                WindowInteractionMode::MoveResize | WindowInteractionMode::Moving => continue,
                WindowInteractionMode::Sizing => {
                    should_continue = true;
                    frame_interval_ms = Some(
                        frame_interval_ms.map_or(INTERACTIVE_RESIZE_FRAME_INTERVAL_MS, |current| {
                            current.min(INTERACTIVE_RESIZE_FRAME_INTERVAL_MS)
                        }),
                    );
                    if window.resize_frame_throttle.advance(elapsed_ms) {
                        repaints.push((HWND(*raw as _), WindowRepaint::Full));
                    }
                    continue;
                }
                WindowInteractionMode::Idle => {}
            }
            let output = window.session.advance(elapsed_ms);
            should_continue |= output.animation_changed;
            if output.animation_changed {
                let interval = window
                    .session
                    .runtime()
                    .frame_interval_ms()
                    .unwrap_or(DEFAULT_FRAME_INTERVAL_MS);
                frame_interval_ms =
                    Some(frame_interval_ms.map_or(interval, |current| current.min(interval)));
            }
            let repaint = if let Some(bounds) = output.dirty_bounds {
                window.session.invalidations_mut().invalidate_rect(bounds);
                WindowRepaint::Rect(bounds)
            } else if output.animation_changed {
                window.session.invalidate_all();
                WindowRepaint::Full
            } else {
                WindowRepaint::None
            };
            if repaint != WindowRepaint::None {
                repaints.push((HWND(*raw as _), repaint));
            }
        }
        (repaints, should_continue, frame_interval_ms)
    });
    for (target, repaint) in repaints {
        request_window_repaint(target, repaint);
    }
    dispatcher.finish_frame_tick_with_interval(
        should_continue,
        frame_interval_ms.unwrap_or(DEFAULT_FRAME_INTERVAL_MS),
    );
}
