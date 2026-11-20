use std::{borrow::Cow, fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StaticUiId(&'static str);

/// A node identity. Ids are long path-like strings that every layer keys its tables by and
/// stores in parent and child lists, so cloning one must not copy the text: an owned id is
/// shared behind an `Arc`.
#[derive(Clone)]
pub struct UiId(IdText);

#[derive(Clone)]
enum IdText {
    Static(&'static str),
    Shared(Arc<str>),
}

impl UiId {
    pub fn new(value: &'static str) -> Self {
        Self(IdText::Static(value))
    }

    pub fn owned(value: impl Into<String>) -> Self {
        Self(IdText::Shared(Arc::from(value.into())))
    }

    pub fn from_parts(parts: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut value = String::new();
        for part in parts {
            if !value.is_empty() {
                value.push('.');
            }
            value.push_str(part.as_ref());
        }
        Self::owned(value)
    }

    pub fn child(&self, segment: impl AsRef<str>) -> Self {
        Self::from_parts([self.as_str(), segment.as_ref()])
    }

    pub fn as_str(&self) -> &str {
        match &self.0 {
            IdText::Static(text) => text,
            IdText::Shared(text) => text,
        }
    }
}

impl From<&'static str> for UiId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

impl From<StaticUiId> for UiId {
    fn from(value: StaticUiId) -> Self {
        Self::new(value.0)
    }
}

impl fmt::Display for UiId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiIdPath {
    segments: Vec<Cow<'static, str>>,
}

impl UiIdPath {
    pub fn new(root: &'static str) -> Self {
        Self {
            segments: vec![Cow::Borrowed(root)],
        }
    }

    pub fn child(&self, segment: impl Into<Cow<'static, str>>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment.into());
        Self { segments }
    }

    pub fn segments(&self) -> &[Cow<'static, str>] {
        &self.segments
    }

    pub fn id(&self, leaf: impl AsRef<str>) -> UiId {
        let mut parts: Vec<&str> = self
            .segments
            .iter()
            .map(|segment| segment.as_ref())
            .collect();
        parts.push(leaf.as_ref());
        UiId::from_parts(parts)
    }
}

impl fmt::Debug for UiId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("UiId").field(&self.as_str()).finish()
    }
}

impl PartialEq for UiId {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (IdText::Shared(a), IdText::Shared(b)) if Arc::ptr_eq(a, b) => true,
            _ => self.as_str() == other.as_str(),
        }
    }
}

impl Eq for UiId {}

impl std::hash::Hash for UiId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}
