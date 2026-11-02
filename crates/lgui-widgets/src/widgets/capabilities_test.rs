use super::text;
use super::*;
use crate::core::*;
use lgui_core::{
    application::{AppView, ApplicationContext},
    session::UiSession,
    window::WindowId,
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const VIEW: UiRect = UiRect::new(0.0, 0.0, 400.0, 240.0);

#[test]
fn semantic_input_changes_route_public_value_and_selection_events() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    let view: AppView = Arc::new(move |cx| {
        let state = cx.state(TextInputModel::new("before"));
        let events = captured.clone();
        text_input(VIEW, state)
            .public_id("editor")
            .on_identified_event(move |event| events.lock().unwrap().push(event))
            .into()
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    let target = id(session.tree(), "editor");
    let output = session.handle_input(InputEvent::Semantic(SemanticInput::SetValue {
        target,
        value: "after".into(),
    }));
    for action in output.default_actions {
        for event in session.handle_default_action(action).action_events {
            event.dispatch(&mut context());
        }
    }
    render(&mut session, &view);
    assert_eq!(
        session
            .tree()
            .public_node("editor")
            .unwrap()
            .semantics
            .value
            .as_deref(),
        Some("after")
    );
    let events = events.lock().unwrap();
    assert!(events.iter().all(|e| e.node.as_ref() == "editor"));
    assert!(events.iter().any(|e| e.payload
        == UiEventPayload::Change {
            value: Some("after".into())
        }));
    assert!(
        events
            .iter()
            .any(|e| e.payload == UiEventPayload::Selection { range: 5..5 })
    );
}

#[test]
fn scrollbar_semantic_value_updates_and_clamps_offset() {
    let slot = Arc::new(Mutex::new(None));
    let captured = slot.clone();
    let view: AppView = Arc::new(move |cx| {
        let offset = cx.state(0.0);
        *captured.lock().unwrap() = Some(offset.clone());
        scrollbar(VIEW, Axis::Vertical, 100.0, 500.0, offset).public_id("scroll")
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    let target = id(session.tree(), "scroll");
    let output = session.handle_input(InputEvent::Semantic(SemanticInput::SetValue {
        target,
        value: "900".into(),
    }));
    for action in output.default_actions {
        for event in session.handle_default_action(action).action_events {
            event.dispatch(&mut context());
        }
    }
    assert_eq!(slot.lock().unwrap().as_ref().unwrap().get(), 400.0);
}
fn render(session: &mut UiSession, view: &AppView) {
    session.render_view(view, VIEW, UiScale::ONE);
}
fn context() -> UiEventContext {
    UiEventContext::new(
        ApplicationContext::empty(crate::memory::test_memory_options()),
        WindowId::new("test"),
    )
}
fn dispatch(tree: &HostTree, id: &UiId, payload: UiEventPayload) {
    let event = tree.handler_event(id, payload).expect("registered handler");
    let mut cx = context();
    for handler in event.capture_handlers.iter().chain(&event.bubble_handlers) {
        handler(&mut cx, &event.payload);
        if cx.propagation_stopped() {
            break;
        }
    }
}
fn key(tree: &HostTree, id: &UiId, key: NamedKey) {
    dispatch(
        tree,
        id,
        UiEventPayload::Keyboard {
            event: KeyboardEvent {
                state: KeyState::Down,
                key: LogicalKey::Named(key),
                ..Default::default()
            },
        },
    );
}
fn id(tree: &HostTree, name: &str) -> UiId {
    tree.nodes()
        .iter()
        .find(|n| n.public_id.as_deref() == Some(name))
        .unwrap()
        .id
        .clone()
}

struct EditingGeometry;
impl lgui_core::text::TextSystem for EditingGeometry {
    fn measure(
        &self,
        r: &lgui_core::text::TextMeasureRequest<'_>,
    ) -> Option<lgui_core::text::TextMetrics> {
        Some(lgui_core::text::TextMetrics {
            width: r.text.len() as f32 * 10.0,
        })
    }
    fn layout(
        &self,
        r: &lgui_core::text::TextLayoutRequest<'_>,
    ) -> Option<lgui_core::text::TextLayout> {
        use lgui_core::text::*;
        let caret = |i: usize| {
            UiRect::new(
                r.bounds.left + i as f32 * 10.0,
                r.bounds.top,
                r.bounds.left + i as f32 * 10.0 + 1.0,
                r.bounds.top + 14.0,
            )
        };
        Some(TextLayout::new(
            r.text.len() as f32 * 10.0,
            14.0,
            false,
            vec![TextLineMetrics {
                range: 0..r.text.len(),
                bounds: r.bounds,
                baseline: r.bounds.top + 12.0,
                hard_break: false,
            }],
            (0..r.text.len())
                .map(|i| TextCluster {
                    range: i..i + 1,
                    bounds: UiRect::new(
                        caret(i).left,
                        r.bounds.top,
                        caret(i + 1).left,
                        r.bounds.top + 14.0,
                    ),
                    direction: TextDirection::LeftToRight,
                })
                .collect(),
            (0..=r.text.len())
                .map(|i| (i, TextAffinity::Downstream, caret(i)))
                .collect(),
        ))
    }
}
#[test]
fn input_pointer_and_ime_geometry_follow_parent_layout_translation() {
    let _text = lgui_core::backend::install_font_environment(
        &ApplicationContext::empty(crate::memory::test_memory_options()),
        lgui_core::text::TextSystemHandle::new(EditingGeometry),
    );
    let slot = Arc::new(Mutex::new(None));
    let captured = slot.clone();
    let view: AppView = Arc::new(move |cx| {
        let state = cx.state(TextInputModel::new("abcd"));
        *captured.lock().unwrap() = Some(state.clone());
        flex(UiRect::new(100.0, 80.0, 300.0, 200.0), Axis::Horizontal)
            .child(text_input(UiRect::new(0.0, 0.0, 120.0, 32.0), state).public_id("editor"))
            .into()
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    let pad = crate::theme::ThemeTokens::default().spacing.sm as f32;
    let output = session.handle_input(InputEvent::PointerDown {
        pointer: PointerData::mouse(Point::new(110.0 + pad, 85.0 + pad)),
        button: PointerButton::Left,
    });
    for event in output.handler_events {
        let mut cx = context();
        for handler in event.capture_handlers.iter().chain(&event.bubble_handlers) {
            handler(&mut cx, &event.payload);
        }
    }
    for action in output.default_actions {
        for event in session.handle_default_action(action).action_events {
            event.dispatch(&mut context());
        }
    }
    assert_eq!(slot.lock().unwrap().as_ref().unwrap().get().cursor, 1);
    render(&mut session, &view);
    let target = id(session.tree(), "editor");
    assert!(
        session
            .tree()
            .node(&target)
            .unwrap()
            .ime_cursor_rect
            .unwrap()
            .left
            >= 100.0
    );
}

#[test]
fn tab_page_direction_keys_do_not_change_the_active_tab() {
    let view: AppView = Arc::new(|cx| {
        let state = cx.state(0usize);
        tabs(
            VIEW,
            vec!["One".into(), "Two".into()].into(),
            state,
            |_, r| {
                group(r)
                    .public_id("page")
                    .on_key_down(|cx, _| cx.prevent_default())
            },
        )
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    key(
        session.tree(),
        &id(session.tree(), "page"),
        NamedKey::ArrowRight,
    );
    render(&mut session, &view);
    assert!(session.tree().nodes().iter().any(|n| {
        n.semantics.as_ref().is_some_and(|s| {
            s.role == SemanticRole::Tab && s.state.selected && s.name.as_deref() == Some("One")
        })
    }));
}
fn input_view(
    read_only: bool,
    password: bool,
    multiline: bool,
    slot: Arc<Mutex<Option<State<TextInputModel>>>>,
    changes: Arc<AtomicUsize>,
) -> AppView {
    Arc::new(move |cx| {
        let state = cx.state(TextInputModel::new("a👩‍💻"));
        *slot.lock().unwrap() = Some(state.clone());
        let changed = changes.clone();
        Element::from(
            text_input(VIEW, state)
                .multiline(multiline)
                .read_only(read_only)
                .password(password)
                .on_change(move |_| {
                    changed.fetch_add(1, Ordering::SeqCst);
                }),
        )
        .public_id("editor")
    })
}
#[test]
fn input_widget_routes_editing_composition_selection_and_callbacks() {
    let slot = Arc::new(Mutex::new(None));
    let changed = Arc::new(AtomicUsize::new(0));
    let view = input_view(false, false, false, slot.clone(), changed.clone());
    let mut session = UiSession::new();
    render(&mut session, &view);
    let target = id(session.tree(), "editor");
    let state = slot.lock().unwrap().clone().unwrap();
    dispatch(session.tree(), &target, UiEventPayload::Focus);
    key(session.tree(), &target, NamedKey::Backspace);
    assert_eq!(state.get().text, "a");
    dispatch(
        session.tree(),
        &target,
        UiEventPayload::CompositionUpdate {
            text: "中文".into(),
            cursor: Some(0..3),
        },
    );
    render(&mut session, &view);
    assert!(
        session
            .tree()
            .nodes()
            .iter()
            .any(|n| n.text.as_deref() == Some("a中文"))
    );
    assert_eq!(state.get().text, "a");
    dispatch(
        session.tree(),
        &target,
        UiEventPayload::Input {
            text: "中\n".into(),
        },
    );
    assert_eq!(state.get().text, "a中");
    assert!(state.get().preedit.is_empty());
    assert_eq!(changed.load(Ordering::SeqCst), 2);
    key(session.tree(), &target, NamedKey::ArrowLeft);
    assert_eq!(state.get().cursor, 1);
    render(&mut session, &view);
    let node = session.tree().public_node("editor").unwrap();
    assert_eq!(node.semantics.value.as_deref(), Some("a中"));
    assert_eq!(node.semantics.text.selection, Some(1..1));
}
#[test]
fn readonly_password_and_multiline_controls_keep_their_contracts() {
    for (readonly, password, multiline) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let slot = Arc::new(Mutex::new(None));
        let view = input_view(
            readonly,
            password,
            multiline,
            slot.clone(),
            Arc::new(AtomicUsize::new(0)),
        );
        let mut session = UiSession::new();
        render(&mut session, &view);
        let target = id(session.tree(), "editor");
        let state = slot.lock().unwrap().clone().unwrap();
        let before = state.get().text;
        dispatch(
            session.tree(),
            &target,
            UiEventPayload::Input {
                text: "x\ny".into(),
            },
        );
        if readonly {
            assert_eq!(state.get().text, before);
        } else if multiline {
            assert!(state.get().text.ends_with("x\ny"));
        } else {
            assert!(state.get().text.ends_with("xy"));
        }
        render(&mut session, &view);
        let sem = session.tree().public_node("editor").unwrap().semantics;
        assert_eq!(sem.state.multiline, multiline);
        assert_eq!(sem.state.read_only, readonly);
        if password {
            assert!(!sem.value.unwrap().contains('a'));
            assert!(
                !session
                    .tree()
                    .nodes()
                    .iter()
                    .any(|n| n.text.as_ref().is_some_and(|s| s.contains("a👩")))
            );
        }
    }
}
#[test]
fn million_row_list_constructs_only_visible_rows_and_scrolls() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(None));
    let c = calls.clone();
    let o = observed.clone();
    let view: AppView = Arc::new(move |cx| {
        let offset = cx.state(0.0);
        *o.lock().unwrap() = Some(offset.clone());
        let c = c.clone();
        virtual_list(
            VIEW,
            Arc::new(RowHeights::uniform(1_000_000, 20.0)),
            offset,
            move |i, r| {
                c.fetch_add(1, Ordering::SeqCst);
                text(
                    r,
                    format!("row {i}"),
                    TextStyle::new(Color::WHITE, 14.0, 400),
                )
                .into()
            },
        )
        .public_id("list")
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    assert!(calls.load(Ordering::SeqCst) <= 14);
    key(session.tree(), &id(session.tree(), "list"), NamedKey::End);
    render(&mut session, &view);
    assert_eq!(
        observed.lock().unwrap().as_ref().unwrap().get(),
        20_000_000.0 - 240.0
    );
    assert!(calls.load(Ordering::SeqCst) <= 28);
    assert!(
        session
            .tree()
            .nodes()
            .iter()
            .any(|n| n.text.as_deref() == Some("row 999999"))
    );
}
#[test]
fn tree_and_tabs_respond_to_keyboard_and_expose_selection() {
    let view: AppView = Arc::new(|cx| {
        let expanded = cx.state(BTreeSet::new());
        let selected = cx.state(Some(Arc::<str>::from("root")));
        let offset = cx.state(0.0);
        tree_view(
            VIEW,
            vec![TreeNode::new("root", "Root").child(TreeNode::new("child", "Child"))].into(),
            expanded,
            selected,
            offset,
        )
        .public_id("tree")
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    let target = id(session.tree(), "tree");
    key(session.tree(), &target, NamedKey::ArrowRight);
    render(&mut session, &view);
    key(session.tree(), &target, NamedKey::ArrowDown);
    render(&mut session, &view);
    assert!(session.tree().nodes().iter().any(|n| {
        n.semantics.as_ref().is_some_and(|s| {
            s.role == SemanticRole::TreeItem
                && s.name.as_deref() == Some("Child")
                && s.state.selected
        })
    }));
    let view: AppView = Arc::new(|cx| {
        let selected = cx.state(0usize);
        tabs(
            VIEW,
            vec!["One".into(), "Two".into()].into(),
            selected,
            |i, r| {
                text(
                    r,
                    format!("page {i}"),
                    TextStyle::new(Color::WHITE, 14.0, 400),
                )
                .into()
            },
        )
        .public_id("tabs")
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    let header = session
        .tree()
        .nodes()
        .iter()
        .find(|n| {
            n.semantics
                .as_ref()
                .is_some_and(|s| s.role == SemanticRole::Tab)
        })
        .unwrap()
        .id
        .clone();
    key(session.tree(), &header, NamedKey::ArrowRight);
    render(&mut session, &view);
    assert!(
        session
            .tree()
            .nodes()
            .iter()
            .any(|n| n.text.as_deref() == Some("page 1"))
    );
    assert!(session.tree().nodes().iter().any(|n| {
        n.semantics.as_ref().is_some_and(|s| {
            s.role == SemanticRole::Tab && s.state.selected && s.name.as_deref() == Some("Two")
        })
    }));
}
#[test]
fn split_drag_updates_ratio_and_preserves_both_minimums() {
    let observed = Arc::new(Mutex::new(None));
    let o = observed.clone();
    let view: AppView = Arc::new(move |cx| {
        let ratio = cx.state(0.5);
        *o.lock().unwrap() = Some(ratio.clone());
        split(
            VIEW,
            Axis::Horizontal,
            ratio,
            (80.0, 90.0),
            |r| group(r).public_id("first"),
            |r| group(r).public_id("second"),
        )
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    for input in [
        InputEvent::PointerDown {
            pointer: PointerData::mouse(Point::new(199.0, 50.0)),
            button: PointerButton::Left,
        },
        InputEvent::PointerMove(PointerData::mouse(Point::new(390.0, 50.0))),
    ] {
        let output = session.handle_input(input);
        for action in output.default_actions {
            for event in session.handle_default_action(action).action_events {
                event.dispatch(&mut context());
            }
        }
    }
    render(&mut session, &view);
    assert!(observed.lock().unwrap().as_ref().unwrap().get() > 0.5);
    assert!(session.tree().public_node("first").unwrap().bounds.width() >= 80.0);
    assert!(session.tree().public_node("second").unwrap().bounds.width() >= 90.0);
}
#[test]
fn presence_runs_through_existing_animation_runtime_and_disables_exit_interaction() {
    let observed = Arc::new(Mutex::new(None));
    let o = observed.clone();
    let view: AppView = Arc::new(move |cx| {
        let visible = cx.state(true);
        *o.lock().unwrap() = Some(visible.clone());
        presence(
            VIEW,
            visible.get(),
            VisibilityTransition {
                duration_ms: 100,
                offset: Point::default(),
            },
            icon_button(VIEW, "owned-key".to_owned(), "Action"),
        )
        .public_id("presence")
    });
    let mut session = UiSession::new();
    render(&mut session, &view);
    session.advance(100.0);
    render(&mut session, &view);
    assert!(session.tree().hit_test(Point::new(10.0, 10.0)).is_some());
    observed.lock().unwrap().as_ref().unwrap().set(false);
    render(&mut session, &view);
    assert!(session.tree().hit_test(Point::new(10.0, 10.0)).is_none());
    session.advance(100.0);
    render(&mut session, &view);
    assert!(
        session
            .tree()
            .public_node("presence")
            .unwrap()
            .semantics
            .state
            .hidden
    );
    let mut tokens = crate::theme::ThemeTokens::default();
    tokens.motion.normal_ms = 37;
    tokens.spacing.sm = 9;
    assert_eq!(VisibilityTransition::from_theme(&tokens).duration_ms, 37);
    assert_eq!(VisibilityTransition::from_theme(&tokens).offset.y, 9.0);
}
