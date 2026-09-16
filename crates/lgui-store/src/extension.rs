use std::{borrow::Cow, sync::Arc};

use lgui_core::{
    application::ApplicationContext,
    core::{UiAsyncContext, UiEventContext},
};

use crate::{StoreRuntime, StoreUnit};

pub trait StoreApplicationExt {
    fn stores(&self) -> Arc<StoreRuntime>;

    fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit;

    fn update_store<T, R>(
        &self,
        reason: impl Into<Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit;
}

impl StoreApplicationExt for ApplicationContext {
    fn stores(&self) -> Arc<StoreRuntime> {
        self.resources()
            .get_or_insert_with(|| StoreRuntime::for_application(self.resources()))
    }

    fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit,
    {
        self.stores().read(read)
    }

    fn update_store<T, R>(
        &self,
        reason: impl Into<Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit,
    {
        self.stores().update(reason, update)
    }
}

#[cfg(test)]
#[path = "extension_test.rs"]
mod tests;

pub trait StoreEventExt {
    fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit;

    fn update_store<T, R>(
        &mut self,
        reason: impl Into<Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit;
}

impl StoreEventExt for UiEventContext {
    fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit,
    {
        self.application().read_store(read)
    }

    fn update_store<T, R>(
        &mut self,
        reason: impl Into<Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit,
    {
        let result = self.application().update_store(reason, update);
        self.mark_changed(false);
        result
    }
}

pub trait StoreAsyncExt {
    fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit;

    fn update_store<T, R>(
        &self,
        reason: impl Into<Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit;
}

impl StoreAsyncExt for UiAsyncContext {
    fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: StoreUnit,
    {
        self.application().read_store(read)
    }

    fn update_store<T, R>(
        &self,
        reason: impl Into<Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: StoreUnit,
    {
        self.application().update_store(reason, update)
    }
}
