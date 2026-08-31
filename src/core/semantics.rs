use std::ops::Range;

use super::{InteractionRole, UiId, UiNode, UiNodeKind, UiRect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SemanticRole {
    #[default]
    Generic,
    Window,
    Group,
    Label,
    Text,
    Image,
    Button,
    Link,
    Navigation,
    List,
    ListItem,
    Table,
    Row,
    Cell,
    CheckBox,
    Switch,
    RadioButton,
    TextInput,
    PasswordInput,
    SearchInput,
    ComboBox,
    ListBoxOption,
    Slider,
    ProgressIndicator,
    Dialog,
    Alert,
    Tab,
    TabList,
    TabPanel,
    Menu,
    MenuItem,
    ScrollView,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticAction {
    Click,
    Focus,
    Blur,
    SetValue,
    Increment,
    Decrement,
    ScrollIntoView,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    SetTextSelection,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticState {
    pub disabled: bool,
    pub selected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub read_only: bool,
    pub required: bool,
    pub busy: bool,
    pub invalid: bool,
    pub hidden: bool,
    pub multiline: bool,
    pub password: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticRelationships {
    pub labelled_by: Vec<UiId>,
    pub described_by: Vec<UiId>,
    pub controls: Vec<UiId>,
    pub owns: Vec<UiId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticText {
    pub selection: Option<Range<usize>>,
    pub character_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Semantics {
    pub role: SemanticRole,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<String>,
    pub numeric_value: Option<f64>,
    pub numeric_min: Option<f64>,
    pub numeric_max: Option<f64>,
    pub numeric_step: Option<f64>,
    pub state: SemanticState,
    pub actions: Vec<SemanticAction>,
    pub relationships: SemanticRelationships,
    pub text: SemanticText,
}

impl Semantics {
    pub fn new(role: SemanticRole) -> Self {
        Self {
            role,
            name: None,
            description: None,
            value: None,
            numeric_value: None,
            numeric_min: None,
            numeric_max: None,
            numeric_step: None,
            state: SemanticState::default(),
            actions: Vec::new(),
            relationships: SemanticRelationships::default(),
            text: SemanticText::default(),
        }
    }

    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = Some(value.into());
        self
    }

    pub fn description(mut self, value: impl Into<String>) -> Self {
        self.description = Some(value.into());
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn action(mut self, action: SemanticAction) -> Self {
        if !self.actions.contains(&action) {
            self.actions.push(action);
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    pub id: UiId,
    pub parent: Option<UiId>,
    pub children: Vec<UiId>,
    pub bounds: UiRect,
    pub semantics: Semantics,
}

impl SemanticNode {
    pub(crate) fn from_ui_node(node: &UiNode) -> Self {
        let mut semantics = node
            .semantics
            .clone()
            .unwrap_or_else(|| inferred_semantics(node));
        if node.event_policy.focus && !semantics.actions.contains(&SemanticAction::Focus) {
            semantics.actions.push(SemanticAction::Focus);
        }
        if (node.click_action.is_some() || node.click_handler.is_some())
            && !semantics.actions.contains(&SemanticAction::Click)
        {
            semantics.actions.push(SemanticAction::Click);
        }
        Self {
            id: node.id.clone(),
            parent: node.parent.clone(),
            children: node.children.clone(),
            bounds: node.hit_rect,
            semantics,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticUpdate {
    pub nodes: Vec<SemanticNode>,
    pub removed: Vec<UiId>,
    pub focus: Option<UiId>,
    pub full: bool,
}

impl SemanticUpdate {
    #[cfg(feature = "accessibility")]
    pub(crate) fn full_from_tree(tree: &super::HostTree, focus: Option<UiId>) -> Self {
        Self {
            nodes: tree
                .nodes()
                .iter()
                .map(|node| SemanticNode::from_ui_node(node))
                .collect(),
            removed: Vec::new(),
            focus,
            full: true,
        }
    }
}

fn inferred_semantics(node: &UiNode) -> Semantics {
    let role = match (node.kind, node.interaction) {
        (UiNodeKind::Root, _) => SemanticRole::Window,
        (UiNodeKind::Text, _) => SemanticRole::Text,
        (UiNodeKind::Image | UiNodeKind::Icon, _) => SemanticRole::Image,
        (UiNodeKind::Button, _) | (_, InteractionRole::Button) => SemanticRole::Button,
        (UiNodeKind::Table, _) => SemanticRole::Table,
        (UiNodeKind::TableRow, _) | (_, InteractionRole::Row) => SemanticRole::Row,
        (_, InteractionRole::Navigation) => SemanticRole::Navigation,
        (_, InteractionRole::DragHandle) => SemanticRole::Slider,
        (UiNodeKind::Group | UiNodeKind::Panel, _) => SemanticRole::Group,
        (_, InteractionRole::Custom(_)) => SemanticRole::Custom,
        _ => SemanticRole::Generic,
    };
    let mut semantics = Semantics::new(role);
    if let Some(text) = node.text.as_ref() {
        if matches!(role, SemanticRole::Text | SemanticRole::Label) {
            semantics.value = Some(text.to_string());
            semantics.text.character_count = Some(text.chars().count());
        } else {
            semantics.name = Some(text.to_string());
        }
    }
    semantics
}
