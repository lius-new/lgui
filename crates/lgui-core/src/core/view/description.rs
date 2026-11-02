//! Runtime-created descriptions reuse the same retained HostTree and layout runtime.
use crate::core::*;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq)]
pub struct NodeProperties {
    pub kind: UiNodeKind,
    pub rect: UiRect,
    pub public_id: Option<Arc<str>>,
    pub style: VisualStyle,
    pub layout: LayoutSpec,
    pub sizing: Option<SizeConstraints>,
    pub item: LayoutItem,
    pub position: Position,
    pub visibility: Visibility,
    pub overflow: Overflow,
    pub z_index: i32,
    pub inert: bool,
    pub text: Option<String>,
    pub text_style: Option<TextStyle>,
    pub icon: Option<Arc<str>>,
    pub semantics: Option<Semantics>,
    pub interaction: InteractionRole,
}
impl NodeProperties {
    pub fn new(kind: UiNodeKind, rect: UiRect) -> Self {
        Self {
            kind,
            rect,
            public_id: None,
            style: VisualStyle::default(),
            layout: LayoutSpec::None,
            sizing: None,
            item: LayoutItem::default(),
            position: Position::Flow,
            visibility: Visibility::Visible,
            overflow: Overflow::Visible,
            z_index: 0,
            inert: false,
            text: None,
            text_style: None,
            icon: None,
            semantics: None,
            interaction: InteractionRole::None,
        }
    }
    fn node(&self, id: UiId, parent: UiId) -> UiNode {
        let mut n = UiNode::new(id, self.kind.clone(), self.rect)
            .parent(parent)
            .style(self.style)
            .layout(self.layout.clone())
            .interaction(self.interaction);
        n.public_id = self.public_id.clone();
        n.size_constraints = self.sizing;
        n.layout_item = self.item;
        n.position = self.position.clone();
        n.visibility = self.visibility;
        n.overflow = self.overflow;
        n.z_index = self.z_index;
        n.inert = self.inert;
        n.text = self.text.clone().map(Cow::Owned);
        n.text_style = self.text_style;
        n.icon_key = self.icon.clone();
        n.semantics = self.semantics.clone();
        n
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct NodeDescription {
    pub key: Arc<str>,
    pub properties: NodeProperties,
    pub children: Vec<NodeDescription>,
}
impl NodeDescription {
    pub fn new(key: impl Into<Arc<str>>, properties: NodeProperties) -> Self {
        Self {
            key: key.into(),
            properties,
            children: Vec::new(),
        }
    }
    pub fn child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DescriptionError {
    MissingMount,
    DuplicateKey(Arc<str>),
    DuplicatePublicId(Arc<str>),
    IdentityCollision,
    EmptyIdentifier,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DescriptionUpdate {
    pub created: usize,
    pub updated: usize,
    pub removed: usize,
    pub reused: usize,
    pub events: Vec<IdentifiedEvent>,
}
#[derive(Clone, PartialEq)]
struct Record {
    props: NodeProperties,
    parent: UiId,
    children: Vec<UiId>,
}
pub struct DescriptionTree {
    namespace: Arc<str>,
    mount: Arc<str>,
    previous: HashMap<UiId, Record>,
}
impl DescriptionTree {
    pub fn new(namespace: impl Into<Arc<str>>, mount: impl Into<Arc<str>>) -> Self {
        Self {
            namespace: namespace.into(),
            mount: mount.into(),
            previous: HashMap::new(),
        }
    }
    pub fn update(
        &mut self,
        tree: &mut HostTree,
        roots: &[NodeDescription],
    ) -> Result<DescriptionUpdate, DescriptionError> {
        let mount = tree
            .public_internal_id(&self.mount)
            .ok_or(DescriptionError::MissingMount)?;
        let mut next = HashMap::new();
        let mut order = Vec::new();
        let mut names = HashSet::new();
        fn flatten(
            ns: &str,
            node: &NodeDescription,
            parent: &UiId,
            next: &mut HashMap<UiId, Record>,
            order: &mut Vec<UiId>,
            names: &mut HashSet<Arc<str>>,
        ) -> Result<UiId, DescriptionError> {
            if node.key.is_empty() {
                return Err(DescriptionError::EmptyIdentifier);
            }
            let id = UiId::owned(format!(
                "description:{}:{ns}:{}:{}",
                ns.len(),
                node.key.len(),
                node.key
            ));
            if next.contains_key(&id) {
                return Err(DescriptionError::DuplicateKey(node.key.clone()));
            }
            if let Some(name) = &node.properties.public_id {
                if name.is_empty() {
                    return Err(DescriptionError::EmptyIdentifier);
                }
                if !names.insert(name.clone()) {
                    return Err(DescriptionError::DuplicatePublicId(name.clone()));
                }
            }
            next.insert(
                id.clone(),
                Record {
                    props: node.properties.clone(),
                    parent: parent.clone(),
                    children: Vec::new(),
                },
            );
            order.push(id.clone());
            let mut children = Vec::new();
            for child in &node.children {
                children.push(flatten(ns, child, &id, next, order, names)?);
            }
            next.get_mut(&id).unwrap().children = children;
            Ok(id)
        }
        let mut root_ids = Vec::new();
        for root in roots {
            root_ids.push(flatten(
                &self.namespace,
                root,
                &mount,
                &mut next,
                &mut order,
                &mut names,
            )?);
        }
        for id in next.keys() {
            if tree.node(id).is_some() && !self.previous.contains_key(id) {
                return Err(DescriptionError::IdentityCollision);
            }
        }
        for n in tree.nodes() {
            if !self.previous.contains_key(&n.id)
                && let Some(name) = &n.public_id
                && names.contains(name)
            {
                return Err(DescriptionError::DuplicatePublicId(name.clone()));
            }
        }
        let before: HashMap<Arc<str>, bool> = self
            .previous
            .keys()
            .filter_map(|id| {
                tree.node(id)
                    .and_then(|n| n.public_id.as_ref())
                    .map(|name| (name.clone(), tree.is_visible(id)))
            })
            .collect();
        let removed: HashSet<_> = self
            .previous
            .keys()
            .filter(|id| !next.contains_key(*id))
            .cloned()
            .collect();
        let mut result = DescriptionUpdate {
            removed: removed.len(),
            ..Default::default()
        };
        tree.remove_ids(&removed);
        for (id, record) in &next {
            if self
                .previous
                .get(id)
                .is_some_and(|r| r.props.public_id != record.props.public_id)
                && let Some(n) = tree.node_mut(id)
            {
                n.public_id = None;
            }
        }
        for id in &order {
            let record = &next[id];
            match self.previous.get(id) {
                Some(old) if old == record => {
                    result.reused += 1;
                    continue;
                }
                Some(_) => result.updated += 1,
                None => result.created += 1,
            };
            if self
                .previous
                .get(id)
                .is_none_or(|r| r.props != record.props || r.parent != record.parent)
            {
                tree.upsert(record.props.node(id.clone(), record.parent.clone()));
            }
        }
        // Inserting a child links it to its parent. Set the authoritative child
        // lists only after all inserts, so new children are never linked twice.
        for id in &order {
            tree.set_children(id, next[id].children.clone());
        }
        let mut children = tree
            .node(&mount)
            .unwrap()
            .children
            .iter()
            .filter(|id| !self.previous.contains_key(*id) && !next.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        children.extend(root_ids);
        tree.set_children(&mount, children);
        if result.created + result.updated + result.removed > 0 {
            tree.reorder_by_hierarchy();
        }
        for (id, record) in &next {
            if let Some(name) = &record.props.public_id {
                let visible = tree.is_visible(id);
                if before.get(name).copied() != Some(visible) {
                    result.events.push(IdentifiedEvent {
                        node: name.clone(),
                        payload: UiEventPayload::Visibility { visible },
                    });
                }
            }
        }
        for (name, visible) in before {
            if visible
                && !next
                    .values()
                    .any(|r| r.props.public_id.as_ref() == Some(&name))
            {
                result.events.push(IdentifiedEvent {
                    node: name,
                    payload: UiEventPayload::Visibility { visible: false },
                });
            }
        }
        self.previous = next;
        Ok(result)
    }
    pub fn clear(&mut self, tree: &mut HostTree) -> Result<DescriptionUpdate, DescriptionError> {
        self.update(tree, &[])
    }
}
/// A runtime description projected through the ordinary retained component tree.
/// Only this small description projection is retained; input, layout, focus and
/// painting continue to belong to the window's existing UiSession.
pub fn described(
    rect: UiRect,
    namespace: impl Into<Arc<str>>,
    nodes: Vec<NodeDescription>,
) -> Element {
    described_with_events(rect, namespace, nodes, None)
}
type DescriptionEvents = Arc<dyn Fn(IdentifiedEvent) + Send + Sync>;
#[derive(Clone)]
struct DescriptionProps {
    rect: UiRect,
    namespace: Arc<str>,
    nodes: Vec<NodeDescription>,
    events: Option<DescriptionEvents>,
}
impl PartialEq for DescriptionProps {
    fn eq(&self, other: &Self) -> bool {
        self.rect == other.rect
            && self.namespace == other.namespace
            && self.nodes == other.nodes
            && match (&self.events, &other.events) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}
pub fn described_with_events(
    rect: UiRect,
    namespace: impl Into<Arc<str>>,
    nodes: Vec<NodeDescription>,
    events: Option<DescriptionEvents>,
) -> Element {
    struct Projection {
        tree: HostTree,
        model: DescriptionTree,
        root: UiId,
        mount: Arc<str>,
    }
    fn element(
        tree: &HostTree,
        id: &UiId,
        events: &Option<DescriptionEvents>,
        root: bool,
    ) -> UiElement {
        let mut node = tree.node(id).expect("description projection node").clone();
        let children = node
            .children
            .iter()
            .map(|id| element(tree, id, events, false))
            .collect::<Vec<_>>();
        if root {
            node.public_id = None;
            node.input_event_handlers.clear();
        }
        if let (Some(name), Some(events)) = (&node.public_id, events) {
            for kind in [
                UiEventKind::Click,
                UiEventKind::Input,
                UiEventKind::Change,
                UiEventKind::Selection,
                UiEventKind::Focus,
                UiEventKind::Blur,
            ] {
                let name = name.clone();
                let events = events.clone();
                node.input_event_handlers.push(UiInputEventBinding {
                    kind,
                    capture: false,
                    handler: Arc::new(move |cx, payload| {
                        cx.stop_propagation();
                        events(IdentifiedEvent {
                            node: name.clone(),
                            payload: payload.clone(),
                        });
                    }),
                });
            }
        }
        UiElement::from_node(node).children(children)
    }
    let namespace = namespace.into();
    let key = namespace.clone();
    component(
        DescriptionProps {
            rect,
            namespace,
            nodes,
            events,
        },
        |cx, props| {
            let projection = cx
                .state_with(|| {
                    let mount: Arc<str> = Arc::from(format!("@description.{}", props.namespace));
                    let root = UiId::owned(format!("description.mount.{}", props.namespace));
                    std::sync::Arc::new(std::sync::Mutex::new(Projection {
                        tree: HostTree::new(),
                        model: DescriptionTree::new(props.namespace.clone(), mount.clone()),
                        root,
                        mount,
                    }))
                })
                .get();
            let mut projection = projection.lock().expect("description projection poisoned");
            let Projection {
                tree,
                model,
                root,
                mount,
            } = &mut *projection;
            tree.upsert(
                UiNode::new(root.clone(), UiNodeKind::Group, props.rect).public_id(mount.clone()),
            );
            model
                .update(tree, &props.nodes)
                .expect("runtime description must have unique keys and public IDs");
            precompiled(element(tree, root, &props.events, true))
        },
    )
    .key(key.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn described_components_preserve_identity_in_the_window_tree() {
        let exposed = Arc::new(std::sync::Mutex::new(None));
        let observed = exposed.clone();
        let view: crate::application::AppView = Arc::new(move |cx| {
            let changed = cx.state(false);
            *observed.lock().unwrap() = Some(changed.clone());
            described(
                UiRect::new(0.0, 0.0, 100.0, 100.0),
                "native",
                vec![
                    desc("a", Color::WHITE),
                    desc(
                        "b",
                        if changed.get() {
                            Color::BLACK
                        } else {
                            Color::WHITE
                        },
                    ),
                ],
            )
        });
        let mut session = crate::session::UiSession::new();
        let viewport = UiRect::new(0.0, 0.0, 100.0, 100.0);
        session.render_view(&view, viewport, UiScale::ONE);
        let node = session
            .tree()
            .nodes()
            .iter()
            .find(|n| n.public_id.as_deref() == Some("a"))
            .unwrap()
            .clone();
        assert_eq!(session.tree().public_roots().len(), 2);
        exposed.lock().unwrap().as_ref().unwrap().set(true);
        session.render_view(&view, viewport, UiScale::ONE);
        let reused = session
            .tree()
            .nodes()
            .iter()
            .find(|n| n.public_id.as_deref() == Some("a"))
            .unwrap();
        assert_eq!(node.id, reused.id);
        assert_eq!(node.style, reused.style);
        assert_eq!(session.tree().public_node("b").unwrap().id.as_ref(), "b");
    }
    fn desc(key: &str, color: Color) -> NodeDescription {
        let mut p = NodeProperties::new(UiNodeKind::Panel, UiRect::new(0.0, 0.0, 20.0, 20.0));
        p.public_id = Some(Arc::from(key));
        p.style = VisualStyle::filled(color);
        NodeDescription::new(key, p)
    }
    #[test]
    fn unchanged_and_reordered_descriptions_keep_identity_and_only_update_changes() {
        let mut t = HostTree::new();
        t.push(
            UiNode::new(
                UiId::owned("host"),
                UiNodeKind::Group,
                UiRect::new(0.0, 0.0, 100.0, 100.0),
            )
            .public_id("mount"),
        );
        let mut d = DescriptionTree::new("demo", "mount");
        let a = desc("a", Color::WHITE);
        let b = desc("b", Color::WHITE);
        assert_eq!(
            d.update(&mut t, &[a.clone(), b.clone()]).unwrap().created,
            2
        );
        let id = t.public_internal_id("a").unwrap();
        assert_eq!(d.update(&mut t, &[b, a]).unwrap().reused, 2);
        assert_eq!(t.public_internal_id("a"), Some(id));
        assert_eq!(t.public_node("mount").unwrap().children[0].as_ref(), "b");
        assert_eq!(d.update(&mut t, &[desc("a", Color(3))]).unwrap().removed, 1);
        assert!(t.public_node("b").is_none());
        assert_eq!(d.clear(&mut t).unwrap().removed, 1);
    }
    #[test]
    fn invalid_descriptions_do_not_partially_mutate_the_tree() {
        let mut t = HostTree::new();
        t.push(
            UiNode::new(
                UiId::owned("host"),
                UiNodeKind::Group,
                UiRect::new(0.0, 0.0, 100.0, 100.0),
            )
            .public_id("mount"),
        );
        let mut d = DescriptionTree::new("demo", "mount");
        let a = desc("a", Color::WHITE);
        d.update(&mut t, std::slice::from_ref(&a)).unwrap();
        assert!(d.update(&mut t, &[a.clone(), a]).is_err());
        assert!(t.public_node("a").is_some());
        assert_eq!(t.nodes().len(), 2);
    }
}
