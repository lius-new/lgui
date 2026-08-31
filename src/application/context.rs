use std::{
    future::Future,
    sync::{Arc, Mutex, RwLock},
};

#[cfg(feature = "router")]
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use crate::{
    command::{Command, CommandHandle, CommandRegistry},
    core::UiTaskSpawner,
    events::{Event, EventBus, EventSubscription},
    memory::{CacheRegistration, DomainRegistration, MemoryGovernor, MemoryOptions},
    resources::Resources,
};

#[cfg(feature = "persistent-cache")]
use crate::memory::PersistentCacheStore;

#[cfg(feature = "router")]
use crate::router::Router;
#[cfg(feature = "notifications")]
use crate::services::NotificationHandle;
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
    memory: MemoryGovernor,
    memory_registrations: Mutex<Vec<CacheRegistration>>,
    #[cfg(feature = "store")]
    stores: Arc<StoreRuntime>,
    #[cfg(feature = "router")]
    routers: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    windows: WindowManager,
}

impl ApplicationContext {
    pub fn empty(memory_options: MemoryOptions) -> Self {
        Self::new(
            Resources::new(),
            None,
            CommandRegistry::default(),
            EventBus::default(),
            memory_options,
        )
    }

    pub(super) fn new(
        resources: Resources,
        executor: Option<UiTaskSpawner>,
        commands: CommandRegistry,
        events: EventBus,
        memory_options: MemoryOptions,
    ) -> Self {
        Self::new_with_memory(
            resources,
            executor,
            commands,
            events,
            memory_options,
            #[cfg(feature = "persistent-cache")]
            None,
        )
    }

    pub(super) fn new_with_memory(
        resources: Resources,
        executor: Option<UiTaskSpawner>,
        commands: CommandRegistry,
        events: EventBus,
        memory_options: MemoryOptions,
        #[cfg(feature = "persistent-cache")] persistent_cache: Option<
            Arc<dyn PersistentCacheStore>,
        >,
    ) -> Self {
        #[cfg(feature = "store")]
        let stores = Arc::new(StoreRuntime::new(resources.clone()));
        let memory = MemoryGovernor::with_store(
            memory_options,
            #[cfg(feature = "persistent-cache")]
            persistent_cache,
        );
        let context = Self {
            inner: Arc::new(ApplicationContextInner {
                resources,
                executor: RwLock::new(executor),
                commands,
                events,
                memory,
                memory_registrations: Mutex::new(Vec::new()),
                #[cfg(feature = "store")]
                stores,
                #[cfg(feature = "router")]
                routers: Mutex::new(HashMap::new()),
                windows: WindowManager::new(),
            }),
        };
        let registration = context
            .memory()
            .register(crate::memory::DomainRegistration::new(
                crate::memory::CacheDomain::ScrollRaster,
                context.memory().next_instance_id(),
                "application:scroll-raster-commands",
                crate::memory::CacheAdapter::managed(
                    crate::core::scroll_raster_command_cache_usage,
                    |request| {
                        let before =
                            crate::core::scroll_raster_command_cache_usage().resident_bytes();
                        crate::core::trim_scroll_raster_command_cache(request.target_bytes);
                        crate::memory::TrimResult {
                            before_bytes: before,
                            after_bytes: crate::core::scroll_raster_command_cache_usage()
                                .resident_bytes(),
                        }
                    },
                    crate::core::set_scroll_raster_command_cache_budget,
                ),
            ));
        context.retain_memory_registration(registration);
        context
    }

    pub fn resources(&self) -> &Resources {
        &self.inner.resources
    }

    pub fn memory(&self) -> &MemoryGovernor {
        &self.inner.memory
    }

    /// Registers an application-owned cache adapter and retains it for this context's lifetime.
    pub fn register_memory_domain(&self, registration: DomainRegistration) {
        let registration = self.memory().register(registration);
        self.retain_memory_registration(registration);
    }

    pub(crate) fn retain_memory_registration(&self, registration: CacheRegistration) {
        self.inner
            .memory_registrations
            .lock()
            .expect("memory registrations poisoned")
            .push(registration);
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
    pub fn clipboard(&self) -> crate::services::ClipboardHandle {
        (*self.resource::<crate::services::ClipboardHandle>()).clone()
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
