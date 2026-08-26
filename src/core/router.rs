use std::{ops::Deref, sync::Arc};

pub struct Navigate<R>(Arc<dyn Fn(R) + Send + Sync + 'static>);

impl<R> Clone for Navigate<R> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<R> Navigate<R> {
    pub fn new(navigate: impl Fn(R) + Send + Sync + 'static) -> Self {
        Self(Arc::new(navigate))
    }
}

impl<R> Deref for Navigate<R> {
    type Target = dyn Fn(R) + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Clone)]
pub struct RouterContext<R> {
    current: R,
    navigate: Navigate<R>,
    navigation_id: u64,
}

impl<R> RouterContext<R> {
    pub fn new(current: R, navigation_id: u64, navigate: Navigate<R>) -> Self {
        Self {
            current,
            navigate,
            navigation_id,
        }
    }

    pub fn current(&self) -> &R {
        &self.current
    }

    pub fn navigate(&self) -> Navigate<R> {
        self.navigate.clone()
    }
}

impl<R: PartialEq> PartialEq for RouterContext<R> {
    fn eq(&self, other: &Self) -> bool {
        self.current == other.current && self.navigation_id == other.navigation_id
    }
}
