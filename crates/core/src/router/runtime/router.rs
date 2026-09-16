use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::super::{Back, Navigate, Replace, RouterContext};

use super::{
    history::{RouteAction, RouteChange},
    snapshot::RouterSnapshot,
    subscription::{RouteListener, RouteSubscriptionToken},
};

static NEXT_ROUTER_ID: AtomicU64 = AtomicU64::new(1);

pub struct Router<R> {
    inner: Arc<RouterInner<R>>,
}

struct RouterInner<R> {
    id: u64,
    state: Mutex<RouterState<R>>,
}

struct RouterState<R> {
    current: R,
    back_stack: Vec<R>,
    revision: u64,
    next_subscription: u64,
    listeners: HashMap<u64, RouteListener<R>>,
}

impl<R> Clone for Router<R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<R> Router<R>
where
    R: Clone + PartialEq + Send + 'static,
{
    pub fn new(initial: R) -> Self {
        Self {
            inner: Arc::new(RouterInner {
                id: NEXT_ROUTER_ID.fetch_add(1, Ordering::Relaxed),
                state: Mutex::new(RouterState {
                    current: initial,
                    back_stack: Vec::new(),
                    revision: 0,
                    next_subscription: 1,
                    listeners: HashMap::new(),
                }),
            }),
        }
    }

    pub fn current(&self) -> R {
        self.inner
            .state
            .lock()
            .expect("router poisoned")
            .current
            .clone()
    }

    pub fn snapshot(&self) -> RouterSnapshot<R> {
        let state = self.inner.state.lock().expect("router poisoned");
        RouterSnapshot {
            current: state.current.clone(),
            revision: state.revision,
            can_back: !state.back_stack.is_empty(),
        }
    }

    pub fn navigate(&self, next: R) -> Option<RouteChange<R>> {
        self.transition(RouteAction::Navigate, move |state| {
            if state.current == next {
                return None;
            }
            let previous = std::mem::replace(&mut state.current, next);
            state.back_stack.push(previous.clone());
            Some(previous)
        })
    }

    pub fn replace(&self, next: R) -> Option<RouteChange<R>> {
        self.transition(RouteAction::Replace, move |state| {
            if state.current == next {
                return None;
            }
            let previous = std::mem::replace(&mut state.current, next);
            while state.back_stack.last() == Some(&state.current) {
                state.back_stack.pop();
            }
            Some(previous)
        })
    }

    pub fn back(&self) -> Option<RouteChange<R>> {
        self.transition(RouteAction::Back, |state| {
            while let Some(previous) = state.back_stack.pop() {
                if previous != state.current {
                    return Some(std::mem::replace(&mut state.current, previous));
                }
            }
            None
        })
    }

    pub fn subscribe(
        &self,
        listener: impl Fn(&RouteChange<R>) + Send + Sync + 'static,
    ) -> RouteSubscriptionToken {
        let mut state = self.inner.state.lock().expect("router poisoned");
        let token = RouteSubscriptionToken {
            router_id: self.inner.id,
            subscription_id: state.next_subscription,
        };
        state.next_subscription += 1;
        state
            .listeners
            .insert(token.subscription_id, Arc::new(listener));
        token
    }

    pub fn unsubscribe(&self, token: RouteSubscriptionToken) -> bool {
        if token.router_id != self.inner.id {
            return false;
        }
        self.inner
            .state
            .lock()
            .expect("router poisoned")
            .listeners
            .remove(&token.subscription_id)
            .is_some()
    }

    pub fn context(&self) -> RouterContext<R> {
        self.context_from(self.snapshot())
    }

    pub(crate) fn observable_id(&self) -> u64 {
        self.inner.id
    }

    pub(crate) fn context_from(&self, snapshot: RouterSnapshot<R>) -> RouterContext<R> {
        let navigate_router = self.clone();
        let replace_router = self.clone();
        let back_router = self.clone();
        RouterContext::new(
            snapshot.current,
            snapshot.revision,
            Navigate::new(move |route| {
                let _ = navigate_router.navigate(route);
            }),
            Replace::new(move |route| {
                let _ = replace_router.replace(route);
            }),
            Back::new(move || {
                let _ = back_router.back();
            }),
        )
    }

    fn transition(
        &self,
        action: RouteAction,
        update: impl FnOnce(&mut RouterState<R>) -> Option<R>,
    ) -> Option<RouteChange<R>> {
        let (change, listeners) = {
            let mut state = self.inner.state.lock().expect("router poisoned");
            let previous = update(&mut state)?;
            state.revision += 1;
            let change = RouteChange {
                previous,
                current: state.current.clone(),
                action,
            };
            let listeners = state.listeners.values().cloned().collect::<Vec<_>>();
            (change, listeners)
        };

        for listener in listeners {
            listener(&change);
        }
        Some(change)
    }
}

#[cfg(test)]
#[path = "router_test.rs"]
mod tests;
