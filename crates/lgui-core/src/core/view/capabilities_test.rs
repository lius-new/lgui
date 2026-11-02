use crate::core::*;
use crate::text::{
    TextLayout, TextLayoutRequest, TextMeasureRequest, TextMetrics, TextSystem, TextSystemHandle,
};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
fn n(id: &str, rect: UiRect) -> UiNode {
    UiNode::new(UiId::owned(id), UiNodeKind::Panel, rect)
}
#[test]
fn outside_pointer_is_direct_and_escape_is_scoped_without_removing_nodes() {
    let r = UiRect::new(0.0, 0.0, 100.0, 100.0);
    let mut t = HostTree::new();
    let handler: UiInputEventHandler = Arc::new(|_, _| {});
    let mut outer = n("outer", r).public_id("outer");
    outer.input_event_handlers.push(UiInputEventBinding {
        kind: UiEventKind::OutsidePointer,
        capture: false,
        handler: handler.clone(),
    });
    t.push(outer);
    let mut inner = n("inner", r)
        .parent(UiId::owned("outer"))
        .public_id("inner");
    inner.focus_scope = true;
    for kind in [UiEventKind::OutsidePointer, UiEventKind::CloseRequested] {
        inner.input_event_handlers.push(UiInputEventBinding {
            kind,
            capture: false,
            handler: handler.clone(),
        });
    }
    t.push(inner);
    let outside = InputEvent::PointerDown {
        pointer: PointerData::mouse(Point::new(150.0, 150.0)),
        button: PointerButton::Left,
    };
    let events = t.behavior_events(&outside, None);
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.bubble_handlers.len() == 1));
    assert!(
        t.behavior_events(
            &InputEvent::PointerDown {
                pointer: PointerData::mouse(Point::new(50.0, 50.0)),
                button: PointerButton::Left
            },
            None
        )
        .is_empty()
    );
    let escape = InputEvent::Keyboard(KeyboardEvent {
        state: KeyState::Down,
        key: LogicalKey::Named(NamedKey::Escape),
        ..Default::default()
    });
    let events = t.behavior_events(&escape, None);
    assert_eq!(events.len(), 1);
    assert_eq!(
        t.identify_handler_event(&events[0]).unwrap().node.as_ref(),
        "inner"
    );
    assert_eq!(t.nodes().len(), 2);
    t.node_mut(&UiId::owned("inner")).unwrap().inert = true;
    assert!(t.behavior_events(&escape, None).is_empty());
}
struct GraphemeMeasure;
impl TextSystem for GraphemeMeasure {
    fn measure(&self, r: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
        Some(TextMetrics {
            width: r.text.graphemes(true).count() as f32 * 10.0,
        })
    }
    fn layout(&self, _: &TextLayoutRequest<'_>) -> Option<TextLayout> {
        None
    }
}
#[test]
fn ellipsis_keeps_whole_graphemes_and_semantic_text_and_reacts_to_resize() {
    let _guard = crate::text::install_text_system(TextSystemHandle::new(GraphemeMeasure));
    let mut t = HostTree::new();
    let make = |width| {
        n("text", UiRect::new(0.0, 0.0, width, 20.0))
            .text("a👩‍💻bc", TextStyle::new(Color::WHITE, 14.0, 400))
            .overflow(Overflow::Ellipsis)
            .public_id("label")
    };
    t.push(make(30.0));
    let mut runtime = LayoutRuntime::new();
    runtime.update(&mut t);
    assert_eq!(
        t.node(&UiId::owned("text"))
            .unwrap()
            .display_text
            .as_deref(),
        Some("a👩‍💻…")
    );
    assert_eq!(
        t.public_node("label").unwrap().semantics.name.as_deref(),
        Some("a👩‍💻bc")
    );
    t.upsert(make(100.0));
    runtime.update(&mut t);
    assert!(t.node(&UiId::owned("text")).unwrap().display_text.is_none());
    t.upsert(make(5.0));
    runtime.update(&mut t);
    assert_eq!(
        t.node(&UiId::owned("text"))
            .unwrap()
            .display_text
            .as_deref(),
        Some("")
    );
}
#[test]
fn dynamic_keys_outlive_their_callers_and_reach_the_scene() {
    let mut t = HostTree::new();
    {
        let key = format!("plugin.{}", "icon");
        let mut node = n("icon", UiRect::new(0.0, 0.0, 20.0, 20.0));
        node.kind = UiNodeKind::Icon;
        node.icon_key = Some(key.into());
        t.push(node);
        let key = format!("plugin.{}", "paint");
        t.push(UiNode::new(
            UiId::owned("custom"),
            UiNodeKind::Custom(key.into()),
            UiRect::new(0.0, 0.0, 20.0, 20.0),
        ));
    }
    let scene = compile_scene(&t);
    assert!(
        scene
            .commands()
            .iter()
            .any(|p| matches!(p,ScenePrimitive::Icon{key,..}if key.as_ref()=="plugin.icon"))
    );
    assert!(
        scene
            .commands()
            .iter()
            .any(|p| matches!(p,ScenePrimitive::Custom{key,..}if key.as_ref()=="plugin.paint"))
    );
}
#[test]
fn description_updates_report_visibility_and_keep_unchanged_nodes_shared() {
    let mut t = HostTree::new();
    t.push(n("mount", UiRect::default()).public_id("mount"));
    let mut d = DescriptionTree::new("test", "mount");
    let mut p = NodeProperties::new(UiNodeKind::Panel, UiRect::default());
    p.public_id = Some(Arc::from("chip"));
    let mut desc = NodeDescription::new("key", p);
    assert_eq!(
        d.update(&mut t, &[desc.clone()]).unwrap().events[0].payload,
        UiEventPayload::Visibility { visible: true }
    );
    let old = t
        .nodes()
        .iter()
        .find(|n| n.public_id.as_deref() == Some("chip"))
        .unwrap()
        .clone();
    assert_eq!(d.update(&mut t, &[desc.clone()]).unwrap().reused, 1);
    let next = t
        .nodes()
        .iter()
        .find(|n| n.public_id.as_deref() == Some("chip"))
        .unwrap();
    assert!(Arc::ptr_eq(&old, next));
    desc.properties.visibility = Visibility::Hidden;
    assert_eq!(
        d.update(&mut t, &[desc]).unwrap().events[0].payload,
        UiEventPayload::Visibility { visible: false }
    );
    assert!(d.clear(&mut t).unwrap().events.is_empty());
}
#[test]
fn selection_and_input_mapping_use_public_ids_without_handlers() {
    let mut t = HostTree::new();
    t.push(n("private", UiRect::default()).public_id("editor"));
    assert_eq!(
        t.identified_events(&UiEvent::TextInput {
            target: UiId::owned("private"),
            text: "中".into()
        })[0]
            .node
            .as_ref(),
        "editor"
    );
    assert_eq!(
        t.identified_payload("editor", UiEventPayload::Selection { range: 1..4 })
            .unwrap()
            .payload,
        UiEventPayload::Selection { range: 1..4 }
    );
}

#[test]
fn hiding_an_ordinary_group_updates_retained_descendant_scenes() {
    let state = Arc::new(std::sync::Mutex::new(None));
    let observed = state.clone();
    let view: crate::application::AppView = Arc::new(move |cx| {
        let visible = cx.state(true);
        *observed.lock().unwrap() = Some(visible.clone());
        group(UiRect::new(0.0, 0.0, 100.0, 100.0))
            .visibility(if visible.get() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            })
            .child(Element::new(|cx| {
                UiElement::panel(
                    cx.id,
                    UiRect::new(0.0, 0.0, 20.0, 20.0),
                    VisualStyle::filled(Color::WHITE),
                )
            }))
    });
    let mut session = crate::session::UiSession::new();
    let viewport = UiRect::new(0.0, 0.0, 100.0, 100.0);
    let first = session.render_view(&view, viewport, UiScale::ONE);
    assert!(!first.scene.commands().is_empty());
    state.lock().unwrap().as_ref().unwrap().set(false);
    let hidden = session.render_view(&view, viewport, UiScale::ONE);
    assert!(hidden.scene.commands().is_empty());
    assert!(hidden.damage.dirty.is_full() || !hidden.damage.dirty.rects().is_empty());
    assert!(
        hidden
            .semantics
            .nodes
            .iter()
            .all(|n| n.semantics.state.hidden)
    );
    state.lock().unwrap().as_ref().unwrap().set(true);
    assert!(
        !session
            .render_view(&view, viewport, UiScale::ONE)
            .scene
            .commands()
            .is_empty()
    );
}
