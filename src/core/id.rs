use std::{borrow::Cow, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StaticUiId(&'static str);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UiId(Cow<'static, str>);

impl UiId {
    pub fn new(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
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
        &self.0
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
