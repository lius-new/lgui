#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouterSnapshot<R> {
    pub(super) current: R,
    pub(super) revision: u64,
    pub(super) can_back: bool,
}

impl<R> RouterSnapshot<R> {
    pub fn current(&self) -> &R {
        &self.current
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_back(&self) -> bool {
        self.can_back
    }
}
