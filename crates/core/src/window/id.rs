#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(String);

impl WindowId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        assert!(!value.trim().is_empty(), "window id must not be empty");
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for WindowId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for WindowId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
