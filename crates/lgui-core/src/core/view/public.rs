//! Optional public labels project a readable tree without exposing internal IDs.
use crate::core::{
    HostTree, SemanticNode, Semantics, UiEvent, UiEventPayload, UiId, UiNode, UiRect,
};
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq)]
pub struct PublicNode {
    pub id: Arc<str>,
    pub parent: Option<Arc<str>>,
    pub children: Vec<Arc<str>>,
    pub bounds: UiRect,
    pub semantics: PublicSemantics,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PublicSemantics {
    pub role: crate::core::SemanticRole,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<String>,
    pub numeric_value: Option<f64>,
    pub state: crate::core::SemanticState,
    pub actions: Vec<crate::core::SemanticAction>,
    pub text: crate::core::SemanticText,
}
impl From<Semantics> for PublicSemantics {
    fn from(s: Semantics) -> Self {
        Self {
            role: s.role,
            name: s.name,
            description: s.description,
            value: s.value,
            numeric_value: s.numeric_value,
            state: s.state,
            actions: s.actions,
            text: s.text,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentifiedEvent {
    pub node: Arc<str>,
    pub payload: UiEventPayload,
}
impl HostTree {
    pub(crate) fn assert_public_ids_unique(&self) {
        let mut names = std::collections::HashSet::new();
        for node in self.nodes() {
            if let Some(name) = &node.public_id {
                assert!(!name.is_empty(), "public node identifier must not be empty");
                assert!(
                    names.insert(name.as_ref()),
                    "duplicate public node identifier: {name}"
                );
            }
        }
    }

    pub(crate) fn assert_public_id_available(&self, node: &UiNode) {
        if let Some(name) = &node.public_id {
            assert!(!name.is_empty(), "public node identifier must not be empty");
            assert!(
                !self
                    .nodes()
                    .iter()
                    .any(|n| n.id != node.id && n.public_id.as_ref() == Some(name)),
                "duplicate public node identifier: {name}"
            );
        }
    }
    pub(crate) fn public_internal_id(&self, name: &str) -> Option<UiId> {
        self.nodes()
            .iter()
            .find(|n| n.public_id.as_deref() == Some(name))
            .map(|n| n.id.clone())
    }
    fn public_ancestor<'a>(&'a self, mut id: Option<&'a UiId>) -> Option<Arc<str>> {
        while let Some(current) = id {
            let n = self.node(current)?;
            if let Some(name) = &n.public_id {
                return Some(name.clone());
            }
            id = n.parent.as_ref();
        }
        None
    }
    pub fn public_node(&self, name: &str) -> Option<PublicNode> {
        let id = self.public_internal_id(name)?;
        let n = self.node(&id)?;
        let mut children = Vec::new();
        fn visit(t: &HostTree, ids: &[UiId], out: &mut Vec<Arc<str>>) {
            for id in ids {
                if let Some(n) = t.node(id) {
                    if let Some(name) = &n.public_id {
                        out.push(name.clone());
                    } else {
                        visit(t, &n.children, out);
                    }
                }
            }
        }
        visit(self, &n.children, &mut children);
        Some(PublicNode {
            id: n.public_id.clone()?,
            parent: self.public_ancestor(n.parent.as_ref()),
            children,
            bounds: n.layout_rect,
            semantics: {
                let mut s = PublicSemantics::from(SemanticNode::from_ui_node(n).semantics);
                s.state.hidden |= !self.is_visible(&n.id);
                s.state.disabled |= self.is_visible(&n.id) && !self.is_interactive(&n.id);
                s
            },
        })
    }
    pub fn public_roots(&self) -> Vec<PublicNode> {
        self.nodes()
            .iter()
            .filter(|n| n.public_id.is_some() && self.public_ancestor(n.parent.as_ref()).is_none())
            .filter_map(|n| self.public_node(n.public_id.as_deref()?))
            .collect()
    }
    pub fn identified_events(&self, event: &UiEvent) -> Vec<IdentifiedEvent> {
        let raw: Vec<(&UiId, UiEventPayload)> = match event {
            UiEvent::Clicked(h) => vec![(&h.id, UiEventPayload::Click)],
            UiEvent::TextInput { target, text } => {
                vec![(target, UiEventPayload::Input { text: text.clone() })]
            }
            UiEvent::Keyboard { target, event } => vec![(
                target,
                UiEventPayload::Keyboard {
                    event: event.clone(),
                },
            )],
            UiEvent::Wheel { hit, delta } => {
                vec![(&hit.id, UiEventPayload::Wheel { delta: *delta })]
            }
            UiEvent::PointerPressed {
                hit,
                pointer,
                button,
            } => vec![(
                &hit.id,
                UiEventPayload::PointerDown {
                    pointer: *pointer,
                    button: *button,
                },
            )],
            UiEvent::PointerMoved { hit, pointer } => {
                vec![(&hit.id, UiEventPayload::PointerMove { pointer: *pointer })]
            }
            UiEvent::PointerDragged { hit, pointer } => {
                vec![(&hit.id, UiEventPayload::PointerDrag { pointer: *pointer })]
            }
            UiEvent::PointerReleased { hit, pointer } => {
                vec![(&hit.id, UiEventPayload::PointerUp { pointer: *pointer })]
            }
            UiEvent::ImeStarted { target } => vec![(target, UiEventPayload::CompositionStart)],
            UiEvent::ImeUpdated {
                target,
                text,
                cursor,
            } => vec![(
                target,
                UiEventPayload::CompositionUpdate {
                    text: text.clone(),
                    cursor: cursor.clone(),
                },
            )],
            UiEvent::ImeEnded { target } => vec![(target, UiEventPayload::CompositionEnd)],
            UiEvent::FocusChanged { previous, current } => previous
                .iter()
                .map(|id| (id, UiEventPayload::Blur))
                .chain(current.iter().map(|h| (&h.id, UiEventPayload::Focus)))
                .collect(),
            _ => Vec::new(),
        };
        raw.into_iter()
            .filter_map(|(target, payload)| {
                Some(IdentifiedEvent {
                    node: self.public_ancestor(Some(target))?,
                    payload,
                })
            })
            .collect()
    }
    pub fn identified_payload(
        &self,
        name: &str,
        payload: UiEventPayload,
    ) -> Option<IdentifiedEvent> {
        self.public_internal_id(name)?;
        Some(IdentifiedEvent {
            node: Arc::from(name),
            payload,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::*;
    fn n(id: &str) -> UiNode {
        UiNode::new(
            UiId::owned(id),
            UiNodeKind::Group,
            UiRect::new(0.0, 0.0, 100.0, 100.0),
        )
    }
    #[test]
    fn named_tree_hides_internal_nodes() {
        let mut t = HostTree::new();
        t.push(n("a").public_id("status"));
        t.push(n("b").parent(UiId::owned("a")));
        t.push(
            n("c")
                .parent(UiId::owned("b"))
                .public_id("chip")
                .semantics(Semantics::new(SemanticRole::Button).name("Ready")),
        );
        assert_eq!(
            t.public_node("status").unwrap().children,
            vec![Arc::<str>::from("chip")]
        );
        let c = t.public_node("chip").unwrap();
        assert_eq!(c.parent.as_deref(), Some("status"));
        assert_eq!(c.semantics.name.as_deref(), Some("Ready"));
        assert!(t.public_node("b").is_none());
    }
    #[test]
    #[should_panic(expected = "duplicate public node identifier")]
    fn duplicates_are_rejected() {
        let mut t = HostTree::new();
        t.push(n("a").public_id("same"));
        t.push(n("b").public_id("same"));
    }
    #[test]
    fn click_returns_public_ancestor() {
        let mut t = HostTree::new();
        t.push(n("a").public_id("chip"));
        t.push(
            n("b")
                .parent(UiId::owned("a"))
                .interaction(InteractionRole::Button),
        );
        let events =
            t.identified_events(&UiEvent::Clicked(t.hit_for_id(&UiId::owned("b")).unwrap()));
        assert_eq!(events[0].node.as_ref(), "chip");
        assert_eq!(events[0].payload, UiEventPayload::Click);
    }
}
