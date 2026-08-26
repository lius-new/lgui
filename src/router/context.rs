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

pub struct Replace<R>(Arc<dyn Fn(R) + Send + Sync + 'static>);

impl<R> Clone for Replace<R> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<R> Replace<R> {
    pub fn new(replace: impl Fn(R) + Send + Sync + 'static) -> Self {
        Self(Arc::new(replace))
    }
}

impl<R> Deref for Replace<R> {
    type Target = dyn Fn(R) + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub struct Back(Arc<dyn Fn() + Send + Sync + 'static>);

impl Clone for Back {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl Back {
    pub fn new(back: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(back))
    }
}

impl Deref for Back {
    type Target = dyn Fn() + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Clone)]
pub struct RouterContext<R> {
    current: R,
    navigate: Navigate<R>,
    replace: Replace<R>,
    back: Back,
    navigation_id: u64,
}

impl<R> RouterContext<R> {
    pub fn new(
        current: R,
        navigation_id: u64,
        navigate: Navigate<R>,
        replace: Replace<R>,
        back: Back,
    ) -> Self {
        Self {
            current,
            navigate,
            replace,
            back,
            navigation_id,
        }
    }

    pub fn current(&self) -> &R {
        &self.current
    }

    pub fn navigation_id(&self) -> u64 {
        self.navigation_id
    }

    pub fn navigate(&self) -> Navigate<R> {
        self.navigate.clone()
    }

    pub fn replace(&self) -> Replace<R> {
        self.replace.clone()
    }

    pub fn back(&self) -> Back {
        self.back.clone()
    }
}

impl<R: PartialEq> PartialEq for RouterContext<R> {
    fn eq(&self, other: &Self) -> bool {
        self.current == other.current && self.navigation_id == other.navigation_id
    }
}
