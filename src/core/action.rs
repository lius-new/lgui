use std::borrow::Cow;

pub const POINTER_DOWN_ACTION: &str = "__ui.pointer.down";
pub const POINTER_DRAG_ACTION: &str = "__ui.pointer.drag";
pub const POINTER_UP_ACTION: &str = "__ui.pointer.up";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionId(Cow<'static, str>);

impl ActionId {
    pub fn new(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for ActionId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAction {
    pub id: ActionId,
    pub payload: Option<Cow<'static, str>>,
}

impl UiAction {
    pub fn new(id: impl Into<ActionId>) -> Self {
        Self {
            id: id.into(),
            payload: None,
        }
    }

    pub fn payload(mut self, payload: impl Into<Cow<'static, str>>) -> Self {
        self.payload = Some(payload.into());
        self
    }

    pub fn id(&self) -> &ActionId {
        &self.id
    }

    pub fn payload_value(&self) -> Option<&str> {
        self.payload.as_deref()
    }
}
