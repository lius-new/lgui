use std::sync::Arc;

use crate::{
    application::AppView,
    core::{component, context_provider, Element, RenderCx},
};

use super::RouterHooks as _;

pub struct Route<R> {
    path: &'static str,
    value: R,
    view: AppView,
}

impl<R> Route<R> {
    pub fn path(&self) -> &'static str {
        self.path
    }
}

pub fn route<R>(
    path: &'static str,
    value: R,
    view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
) -> Route<R> {
    assert!(path.starts_with('/'), "route paths must start with `/`");
    Route {
        path,
        value,
        view: Arc::new(view),
    }
}

pub trait IntoRoutes<R> {
    fn into_routes(self) -> Vec<Route<R>>;
}

impl<R> IntoRoutes<R> for Route<R> {
    fn into_routes(self) -> Vec<Route<R>> {
        vec![self]
    }
}

macro_rules! tuple_routes {
    ($($name:ident),+) => {
        impl<R, $($name),+> IntoRoutes<R> for ($($name,)+)
        where
            $($name: IntoRoutes<R>,)+
        {
            fn into_routes(self) -> Vec<Route<R>> {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                let mut routes = Vec::new();
                $(routes.extend($name.into_routes());)+
                routes
            }
        }
    };
}

tuple_routes!(A, B);
tuple_routes!(A, B, C);
tuple_routes!(A, B, C, D);
tuple_routes!(A, B, C, D, E);
tuple_routes!(A, B, C, D, E, F);
tuple_routes!(A, B, C, D, E, F, G);
tuple_routes!(A, B, C, D, E, F, G, H);
tuple_routes!(A, B, C, D, E, F, G, H, I);
tuple_routes!(A, B, C, D, E, F, G, H, I, J);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K);
tuple_routes!(A, B, C, D, E, F, G, H, I, J, K, L);

pub struct DeclarativeRouter<R> {
    routes: Arc<Vec<Route<R>>>,
}

impl<R> Clone for DeclarativeRouter<R> {
    fn clone(&self) -> Self {
        Self {
            routes: Arc::clone(&self.routes),
        }
    }
}

pub fn create_router<R>(routes: impl IntoRoutes<R>) -> DeclarativeRouter<R>
where
    R: Clone + PartialEq,
{
    let routes = routes.into_routes();
    assert!(
        !routes.is_empty(),
        "a router must define at least one route"
    );
    for (index, route) in routes.iter().enumerate() {
        assert!(
            routes[..index].iter().all(|entry| entry.path != route.path),
            "duplicate route path `{}`",
            route.path
        );
        assert!(
            routes[..index]
                .iter()
                .all(|entry| entry.value != route.value),
            "duplicate typed route"
        );
    }
    DeclarativeRouter {
        routes: Arc::new(routes),
    }
}

impl<R> DeclarativeRouter<R>
where
    R: Default + Clone + PartialEq + Send + Sync + 'static,
{
    pub fn outlet(&self, cx: &mut RenderCx<'_, '_>) -> Element {
        let router = cx.application().router::<R>();
        let context = cx.use_router(router);
        let current = context.current().clone();
        let selected = self
            .routes
            .iter()
            .find(|entry| entry.value == current)
            .unwrap_or_else(|| panic!("current route has no declarative component"));
        let view = Arc::clone(&selected.view);
        let viewport = cx.viewport();
        context_provider(
            context,
            component(viewport, move |cx, _| view(cx)).key(selected.path),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum Page {
        #[default]
        Home,
        About,
    }

    #[test]
    fn declarative_routes_reject_duplicate_paths() {
        let result = std::panic::catch_unwind(|| {
            create_router((
                route("/", Page::Home, |_| crate::core::content_text("home")),
                route("/", Page::About, |_| crate::core::content_text("about")),
            ))
        });
        assert!(result.is_err());
    }
}
