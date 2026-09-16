use crate::core::{Observable, ObservableListener, RenderCx, UiEffect};

use super::{Back, Navigate, Replace, Router, RouterContext, RouterSnapshot};

pub trait RouterHooks {
    fn use_route<R>(&mut self) -> R
    where
        R: Clone + PartialEq + 'static;

    fn use_navigate<R>(&mut self) -> Navigate<R>
    where
        R: Clone + PartialEq + 'static;

    fn use_replace<R>(&mut self) -> Replace<R>
    where
        R: Clone + PartialEq + 'static;

    fn use_back<R>(&mut self) -> Back
    where
        R: Clone + PartialEq + 'static;

    fn use_router<R>(&mut self, router: Router<R>) -> RouterContext<R>
    where
        R: Clone + PartialEq + Send + 'static;

    fn use_router_snapshot<R>(&mut self, router: Router<R>) -> RouterSnapshot<R>
    where
        R: Clone + PartialEq + Send + 'static;
}

impl RouterHooks for RenderCx<'_, '_> {
    fn use_route<R>(&mut self) -> R
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().current().clone()
    }

    fn use_navigate<R>(&mut self) -> Navigate<R>
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().navigate()
    }

    fn use_replace<R>(&mut self) -> Replace<R>
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().replace()
    }

    fn use_back<R>(&mut self) -> Back
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().back()
    }

    fn use_router<R>(&mut self, router: Router<R>) -> RouterContext<R>
    where
        R: Clone + PartialEq + Send + 'static,
    {
        let snapshot = use_snapshot(self, router.clone());
        router.context_from(snapshot)
    }

    fn use_router_snapshot<R>(&mut self, router: Router<R>) -> RouterSnapshot<R>
    where
        R: Clone + PartialEq + Send + 'static,
    {
        use_snapshot(self, router)
    }
}

fn use_snapshot<R>(cx: &mut RenderCx<'_, '_>, router: Router<R>) -> RouterSnapshot<R>
where
    R: Clone + PartialEq + Send + 'static,
{
    let read_router = router.clone();
    let subscribe_router = router.clone();
    let source = Observable::new(
        router.observable_id(),
        move || read_router.snapshot(),
        move |listener| subscribe(subscribe_router.clone(), listener),
    );
    cx.use_observable(source, clone_snapshot::<R>)
}

fn subscribe<R>(router: Router<R>, listener: ObservableListener) -> UiEffect
where
    R: Clone + PartialEq + Send + 'static,
{
    let cleanup_router = router.clone();
    let token = router.subscribe(move |_| listener());
    Box::new(move || {
        cleanup_router.unsubscribe(token);
    })
}

fn clone_snapshot<R: Clone>(snapshot: &RouterSnapshot<R>) -> RouterSnapshot<R> {
    snapshot.clone()
}

#[cfg(test)]
#[path = "hooks_test.rs"]
mod tests;
