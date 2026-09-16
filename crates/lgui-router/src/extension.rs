use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, Mutex},
};

use lgui_core::{
    application::ApplicationContext,
    core::{UiAsyncContext, UiEventContext},
};

use crate::Router;

#[derive(Default)]
struct RouterRegistry {
    values: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

pub trait RouterApplicationExt {
    fn router<R>(&self) -> Router<R>
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;
}

impl RouterApplicationExt for ApplicationContext {
    fn router<R>(&self) -> Router<R>
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        let registry = self.resources().get_or_insert_with(RouterRegistry::default);
        let mut routers = registry.values.lock().expect("router registry poisoned");
        routers
            .entry(TypeId::of::<R>())
            .or_insert_with(|| Arc::new(Router::new(R::default())))
            .clone()
            .downcast::<Router<R>>()
            .unwrap_or_else(|_| panic!("router registry type mismatch"))
            .as_ref()
            .clone()
    }
}

pub trait RouterEventExt {
    fn navigate<R>(&mut self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;

    fn replace<R>(&mut self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;

    fn back<R>(&mut self)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;
}

impl RouterEventExt for UiEventContext {
    fn navigate<R>(&mut self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        if self.application().router::<R>().navigate(route).is_some() {
            self.mark_changed(true);
        }
    }

    fn replace<R>(&mut self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        if self.application().router::<R>().replace(route).is_some() {
            self.mark_changed(true);
        }
    }

    fn back<R>(&mut self)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        if self.application().router::<R>().back().is_some() {
            self.mark_changed(true);
        }
    }
}

pub trait RouterAsyncExt {
    fn navigate<R>(&self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;

    fn replace<R>(&self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;

    fn back<R>(&self)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static;
}

impl RouterAsyncExt for UiAsyncContext {
    fn navigate<R>(&self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        self.application().router::<R>().navigate(route);
    }

    fn replace<R>(&self, route: R)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        self.application().router::<R>().replace(route);
    }

    fn back<R>(&self)
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        self.application().router::<R>().back();
    }
}
