use super::*;
use crate::{
    application::AppView,
    core::{
        component, group, text, Color, Element, ElementRenderCx, InputEvent, InteractionRole,
        Point, PointerButton, PointerData, State, TextStyle, UiElement, UiId, VisualStyle,
    },
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[test]
fn full_invalidation_marks_retained_components_dirty() {
    let mut session = UiSession::new();
    let components = session.runtime().component_tree();
    components.begin_render();
    let root = components.root(UiId::owned("session-root"), "session-root");
    components.begin_component_execution(root);
    components.finish_component(root);
    components.end_render();
    assert!(!components.is_dirty(root));

    session.invalidate_all();

    assert!(session.runtime().component_tree().is_dirty(root));
}

#[test]
fn suspending_rendering_releases_the_host_and_preserves_component_state() {
    let mut session = UiSession::new();
    let components = session.runtime().component_tree();
    components.begin_render();
    let root = components.root(UiId::owned("session-root"), "session-root");
    components.begin_component_execution(root);
    components.finish_component(root);
    components.end_render();

    session.suspend_rendering();

    assert!(!session.has_tree());
    assert!(session.runtime().component_tree().is_alive(root));
    assert!(session.runtime().component_tree().is_dirty(root));
}

#[test]
fn state_update_uses_host_diff_damage_instead_of_invalidating_the_component_bounds() {
    let state = Arc::new(Mutex::new(None::<State<i32>>));
    let captured = Arc::clone(&state);
    let view: AppView = Arc::new(move |cx| {
        let count = cx.state(0_i32);
        *captured.lock().expect("captured state poisoned") = Some(count.clone());
        group(UiRect::new(0.0, 0.0, 400.0, 300.0)).content((
            text(
                UiRect::new(20.0, 20.0, 180.0, 60.0),
                "unchanged",
                TextStyle::new(Color::WHITE, 18.0, 400),
            ),
            text(
                UiRect::new(20.0, 80.0, 180.0, 120.0),
                format!("count={}", count.get()),
                TextStyle::new(Color::WHITE, 18.0, 400),
            ),
        ))
    });
    let viewport = UiRect::new(0.0, 0.0, 400.0, 300.0);
    let mut session = UiSession::new();

    let first = session.render_view(&view, viewport, UiScale::ONE);
    assert!(first.damage.dirty.is_full());

    state
        .lock()
        .expect("captured state poisoned")
        .as_ref()
        .expect("state handle missing")
        .update(|count| *count += 1);
    let pending = session.apply_pending_updates();
    assert_eq!(pending.dirty_ids.len(), 1);

    let second = session.render_view(&view, viewport, UiScale::ONE);

    assert!(!second.damage.dirty.is_full());
    assert!(!second.damage.dirty.is_empty());
    assert!(second.damage.dirty.dirty_area() < second.damage.dirty.viewport_area() / 2.0);
    assert!(second.damage.dirty.effective_rects().iter().all(|rect| rect
        .intersect(UiRect::new(20.0, 20.0, 180.0, 60.0))
        .is_none()));
}

#[test]
fn pointer_focus_reexecutes_components_losing_and_gaining_focus() {
    let first_executions = Arc::new(AtomicUsize::new(0));
    let second_executions = Arc::new(AtomicUsize::new(0));
    let first_observed_focus = Arc::new(Mutex::new(Vec::new()));
    let second_observed_focus = Arc::new(Mutex::new(Vec::new()));
    let first_id = Arc::new(Mutex::new(None::<UiId>));
    let second_id = Arc::new(Mutex::new(None::<UiId>));
    let view: AppView = Arc::new({
        let first_executions = Arc::clone(&first_executions);
        let second_executions = Arc::clone(&second_executions);
        let first_observed_focus = Arc::clone(&first_observed_focus);
        let second_observed_focus = Arc::clone(&second_observed_focus);
        let first_id = Arc::clone(&first_id);
        let second_id = Arc::clone(&second_id);
        move |_cx| {
            let field = |rect: UiRect,
                         executions: Arc<AtomicUsize>,
                         observed_focus: Arc<Mutex<Vec<bool>>>,
                         field_id: Arc<Mutex<Option<UiId>>>| {
                component((), move |_cx, _props| {
                    executions.fetch_add(1, Ordering::SeqCst);
                    Element::new(move |cx: ElementRenderCx<'_, '_, '_>| {
                        let focused = cx.context.interaction_flags(&cx.id).focused;
                        observed_focus
                            .lock()
                            .expect("observed focus poisoned")
                            .push(focused);
                        *field_id.lock().expect("field id poisoned") = Some(cx.id.clone());
                        UiElement::panel(
                            cx.id,
                            rect,
                            VisualStyle::filled(if focused {
                                Color(0x00FF00)
                            } else {
                                Color(0xFF0000)
                            }),
                        )
                        .interaction(InteractionRole::Button)
                    })
                })
            };
            group(UiRect::new(0.0, 0.0, 240.0, 120.0)).content((
                field(
                    UiRect::new(0.0, 0.0, 120.0, 40.0),
                    Arc::clone(&first_executions),
                    Arc::clone(&first_observed_focus),
                    Arc::clone(&first_id),
                ),
                field(
                    UiRect::new(0.0, 60.0, 120.0, 100.0),
                    Arc::clone(&second_executions),
                    Arc::clone(&second_observed_focus),
                    Arc::clone(&second_id),
                ),
            ))
        }
    });
    let viewport = UiRect::new(0.0, 0.0, 240.0, 120.0);
    let mut session = UiSession::new();

    session.render_view(&view, viewport, UiScale::ONE);
    let first_id = first_id
        .lock()
        .expect("first id poisoned")
        .clone()
        .expect("first id missing");
    let second_id = second_id
        .lock()
        .expect("second id poisoned")
        .clone()
        .expect("second id missing");

    click(&mut session, Point::new(10.0, 10.0));
    assert_eq!(
        session.runtime().interaction_state().focused,
        Some(first_id)
    );
    session.render_view(&view, viewport, UiScale::ONE);

    click(&mut session, Point::new(10.0, 70.0));
    assert_eq!(
        session.runtime().interaction_state().focused,
        Some(second_id)
    );
    session.render_view(&view, viewport, UiScale::ONE);

    assert_eq!(first_executions.load(Ordering::SeqCst), 3);
    assert_eq!(second_executions.load(Ordering::SeqCst), 2);
    assert_eq!(
        *first_observed_focus
            .lock()
            .expect("first observed focus poisoned"),
        vec![false, true, false]
    );
    assert_eq!(
        *second_observed_focus
            .lock()
            .expect("second observed focus poisoned"),
        vec![false, true]
    );
}

fn click(session: &mut UiSession, point: Point) {
    for input in [
        InputEvent::PointerDown {
            pointer: PointerData::mouse(point),
            button: PointerButton::Left,
        },
        InputEvent::PointerUp {
            pointer: PointerData::mouse(point),
            button: PointerButton::Left,
        },
    ] {
        let output = session.handle_input(input);
        if let Some(bounds) = output.dirty_bounds {
            session.invalidations_mut().invalidate_rect(bounds);
        }
    }
}
