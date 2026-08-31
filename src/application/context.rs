use std::{
    future::Future,
    sync::{Arc, RwLock},
};

#[cfg(feature = "router")]
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Mutex,
};

use crate::{
    command::{Command, CommandHandle, CommandRegistry},
    core::UiTaskSpawner,
    events::{Event, EventBus, EventSubscription},
    resources::Resources,
};

#[cfg(feature = "notifications")]
use crate::platform::NotificationHandle;
#[cfg(feature = "router")]
use crate::router::Router;
#[cfg(feature = "store")]
use crate::store::StoreRuntime;

use super::{RenderError, RenderErrorRegistration, WindowManager};

#[derive(Clone)]
pub struct ApplicationContext {
    inner: Arc<ApplicationContextInner>,
}

struct ApplicationContextInner {
    resources: Resources,
    executor: RwLock<Option<UiTaskSpawner>>,
    commands: CommandRegistry,
    events: EventBus,
    #[cfg(feature = "store")]
    stores: Arc<StoreRuntime>,
    #[cfg(feature = "router")]
    routers: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    windows: WindowManager,
}

impl ApplicationContext {
    pub fn empty() -> Self {
        Self::new(
            Resources::new(),
            None,
            CommandRegistry::default(),
            EventBus::default(),
        )
    }

    pub(super) fn new(
        resources: Resources,
        executor: Option<UiTaskSpawner>,
        commands: CommandRegistry,
        events: EventBus,
    ) -> Self {
        #[cfg(feature = "store")]
        let stores = Arc::new(StoreRuntime::new(resources.clone()));
        Self {
            inner: Arc::new(ApplicationContextInner {
                resources,
                executor: RwLock::new(executor),
                commands,
                events,
                #[cfg(feature = "store")]
                stores,
                #[cfg(feature = "router")]
                routers: Mutex::new(HashMap::new()),
                windows: WindowManager::new(),
            }),
        }
    }

    pub fn resources(&self) -> &Resources {
        &self.inner.resources
    }

    pub fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.inner.resources.require::<T>()
    }

    pub fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.inner.resources.get::<T>()
    }

    pub fn command<C>(&self) -> CommandHandle<C>
    where
        C: Command,
    {
        CommandHandle::new(self.clone())
    }

    pub async fn invoke<C>(&self, args: C::Args) -> Result<C::Output, C::Error>
    where
        C: Command,
    {
        self.command::<C>().invoke(args).await
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.inner.events.emit(event)
    }

    pub fn subscribe<E>(&self, listener: impl Fn(E) + Send + Sync + 'static) -> EventSubscription
    where
        E: Event,
    {
        self.inner.events.subscribe(listener)
    }

    pub(crate) fn command_registry(&self) -> &CommandRegistry {
        &self.inner.commands
    }

    #[cfg(feature = "store")]
    pub fn stores(&self) -> &Arc<StoreRuntime> {
        &self.inner.stores
    }

    #[cfg(feature = "store")]
    pub fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.inner.stores.read(read)
    }

    #[cfg(feature = "store")]
    pub fn update_store<T, R>(
        &self,
        reason: impl Into<std::borrow::Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.inner.stores.update(reason, update)
    }

    #[cfg(feature = "router")]
    pub fn router<R>(&self) -> Router<R>
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        let mut routers = self.inner.routers.lock().expect("router registry poisoned");
        routers
            .entry(TypeId::of::<R>())
            .or_insert_with(|| Arc::new(Router::new(R::default())))
            .clone()
            .downcast::<Router<R>>()
            .unwrap_or_else(|_| panic!("router registry type mismatch"))
            .as_ref()
            .clone()
    }

    pub fn set_executor(&self, executor: UiTaskSpawner) {
        *self.inner.executor.write().expect("UI executor poisoned") = Some(executor);
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) -> bool {
        let Some(executor) = self
            .inner
            .executor
            .read()
            .expect("UI executor poisoned")
            .clone()
        else {
            return false;
        };
        executor.spawn(Box::pin(task));
        true
    }

    pub fn windows(&self) -> WindowManager {
        self.inner.windows.clone()
    }

    pub(crate) fn report_render_error(&self, error: RenderError) {
        if let Some(registration) = self.try_resource::<RenderErrorRegistration>() {
            registration.report(&error);
        } else {
            eprintln!("{error}");
        }
    }

    #[cfg(feature = "notifications")]
    pub fn notifications(&self) -> Option<Arc<NotificationHandle>> {
        self.try_resource::<NotificationHandle>()
    }

    #[cfg(feature = "clipboard")]
    pub fn clipboard(&self) -> crate::platform::ClipboardHandle {
        (*self.resource::<crate::platform::ClipboardHandle>()).clone()
    }

    #[cfg(feature = "open-url")]
    pub fn open_url(&self, url: &str) -> Result<(), crate::desktop::OpenUrlError> {
        self.resource::<crate::desktop::OpenUrlHandle>().open(url)
    }

    #[cfg(feature = "dialogs")]
    pub fn file_dialogs(&self) -> Arc<crate::dialogs::FileDialogHandle> {
        self.resource::<crate::dialogs::FileDialogHandle>()
    }

    pub(crate) fn task_spawner(&self) -> Option<UiTaskSpawner> {
        self.inner
            .executor
            .read()
            .expect("UI executor poisoned")
            .clone()
    }
}

impl PartialEq for ApplicationContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}
