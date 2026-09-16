use std::{
    future::Future,
    sync::{Arc, Mutex, RwLock},
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

use super::{ApplicationScopeFuture, RenderError, RenderErrorRegistration, WindowManager};

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
        self.scope(self.command::<C>().invoke(args)).await
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.emit_keyed(&crate::events::EventKey::new(E::NAME), event)
    }

    pub fn subscribe<E>(&self, listener: impl Fn(E) + Send + Sync + 'static) -> EventSubscription
    where
        E: Event,
    {
        self.subscribe_keyed(crate::events::EventKey::new(E::NAME), listener)
    }

    pub fn subscribe_keyed<T>(
        &self,
        key: crate::events::EventKey<T>,
        listener: impl Fn(T) + Send + Sync + 'static,
    ) -> EventSubscription
    where
        T: Clone + Send + Sync + 'static,
    {
        self.inner.events.subscribe(key, listener)
    }

    pub(crate) fn emit_keyed<T>(&self, key: &crate::events::EventKey<T>, payload: T) -> usize
    where
        T: Clone + Send + Sync + 'static,
    {
        self.inner.events.emit(key, payload)
    }

    pub(crate) fn command_registry(&self) -> &CommandRegistry {
        &self.inner.commands
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
        executor.spawn(Box::pin(self.scope(task)));
        true
    }

    /// Runs a Future with this Application as the active context on every poll.
    ///
    /// LGUI task APIs apply this automatically. Use `scope` when an
    /// application-owned Future is submitted to an external executor.
    pub fn scope<F>(&self, future: F) -> impl Future<Output = F::Output>
    where
        F: Future,
    {
        ApplicationScopeFuture::new(self.clone(), future)
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

    pub(crate) fn task_spawner(&self) -> Option<UiTaskSpawner> {
        let executor = self
            .inner
            .executor
            .read()
            .expect("UI executor poisoned")
            .clone()?;
        let application = Arc::downgrade(&self.inner);
        Some(Arc::new(move |task: crate::core::UiTask| {
            let Some(inner) = application.upgrade() else {
                return;
            };
            let application = ApplicationContext { inner };
            executor.spawn(Box::pin(application.scope(task)));
        }))
    }
}

impl PartialEq for ApplicationContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}
