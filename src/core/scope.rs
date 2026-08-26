use super::{UiId, UiIdPath};

#[derive(Clone, Debug)]
pub struct UiScope {
    path: UiIdPath,
}

impl UiScope {
    pub fn new(root: &'static str) -> Self {
        Self {
            path: UiIdPath::new(root),
        }
    }

    pub fn child(&self, segment: &'static str) -> Self {
        Self {
            path: self.path.child(segment),
        }
    }

    pub fn from_path(path: UiIdPath) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &UiIdPath {
        &self.path
    }

    pub fn id(&self, leaf: impl AsRef<str>) -> UiId {
        self.path.id(leaf)
    }

    pub(crate) fn node_id(&self) -> UiId {
        self.path.id("n")
    }
}
