use std::{
    any::type_name,
    ops::Deref,
    sync::{Arc, Mutex},
};

#[cfg(feature = "async")]
use std::future::Future;

#[cfg(feature = "router")]
use super::{Back, Navigate, Replace, RouterContext};
use super::{
    ComponentId, ComponentState, DeclarativeView, HookId, HookSlotKind, Observable, UiElement,
    UiId, UiRenderContext, UiScope,
};
use crate::{
    command::{Command, CommandHandle},
    events::{AsyncEventHandler, Event},
};

pub struct RenderCx<'a, 'ctx> {
    scope: UiScope,
    context: &'ctx UiRenderContext<'a>,
    component_id: ComponentId,
    hook_index: usize,
    force_children: bool,
}

#[derive(Clone)]
pub struct State<T> {
    value: Arc<Mutex<T>>,
    invalidate: Arc<dyn Fn() + Send + Sync + 'static>,
}

#[derive(Clone)]
pub struct StateSetter<T> {
    set: Arc<dyn Fn(T) + Send + Sync + 'static>,
    state: State<T>,
}

#[derive(Clone)]
pub struct UiFocusHandle {
    target: UiId,
    updates: Arc<super::UiUpdateQueue>,
}

#[derive(Clone, Copy)]
enum HookKind {
    Stable,
    State,
    Effect,
    Context,
    Store,
}

impl<'a, 'ctx> RenderCx<'a, 'ctx> {
    pub(crate) const fn component_id(&self) -> ComponentId {
        self.component_id
    }

    pub fn new(scope: &UiScope, context: &'ctx UiRenderContext<'a>) -> Self {
        let component_id = context
            .component_tree()
            .root(scope.node_id(), "RenderCx root");
        context.contexts().begin_component(component_id);
        let force_children = context
            .component_tree()
            .begin_component_execution(component_id);
        Self {
            scope: scope.clone(),
            context,
            component_id,
            hook_index: 0,
            force_children,
        }
    }

    #[doc(hidden)]
    pub fn use_stable_id(&mut self) -> UiId {
        let index = self.next_hook(HookKind::Stable).index();
        self.scope
            .id(format!("h.{}.{index}", HookKind::Stable.code()))
    }

    #[doc(hidden)]
    pub fn focus_handle(&self, target: UiId) -> UiFocusHandle {
        UiFocusHandle {
            target,
            updates: self.context.hook_updates(),
        }
    }

    pub(crate) fn for_component(
        scope: &UiScope,
        context: &'ctx UiRenderContext<'a>,
        component_id: ComponentId,
        force_children: bool,
    ) -> Self {
        Self {
            scope: scope.clone(),
            context,
            component_id,
            hook_index: 0,
            force_children,
        }
    }

    pub fn node_id(&self) -> UiId {
        self.scope.node_id()
    }

    pub fn viewport(&self) -> super::UiRect {
        self.context.viewport()
    }

    pub fn application(&mut self) -> crate::application::ApplicationContext {
        self.use_context::<crate::application::ApplicationContext>()
    }

    pub fn command<C>(&mut self) -> CommandHandle<C>
    where
        C: Command,
    {
        self.application().command::<C>()
    }

    pub fn use_event<E>(
        &mut self,
        deps: impl Clone + PartialEq + 'static,
        handler: impl Fn(E) + Send + Sync + 'static,
    ) where
        E: Event,
    {
        let application = self.application();
        self.use_effect(deps, move || {
            let subscription = application.subscribe::<E>(handler);
            move || drop(subscription)
        });
    }

    pub fn use_event_once<E>(&mut self, handler: impl Fn(E) + Send + Sync + 'static)
    where
        E: Event,
    {
        self.use_event::<E>((), handler);
    }

    pub fn use_event_async<E>(
        &mut self,
        deps: impl Clone + PartialEq + 'static,
        handler: impl AsyncEventHandler<E>,
    ) where
        E: Event,
    {
        let application = self.application();
        let handler = Arc::new(handler);
        self.use_effect(deps, move || {
            let task_application = application.clone();
            let subscription = application.subscribe::<E>(move |event| {
                let context = super::UiAsyncContext::application_only(task_application.clone());
                let _ = task_application.spawn(handler.call(context, event));
            });
            move || drop(subscription)
        });
    }

    pub fn use_event_async_once<E>(&mut self, handler: impl AsyncEventHandler<E>)
    where
        E: Event,
    {
        self.use_event_async::<E>((), handler);
    }

    pub(crate) fn compile<V>(&self, view: V) -> UiElement
    where
        V: DeclarativeView,
    {
        view.compile(
            &self.scope,
            self.context,
            self.component_id,
            self.force_children,
        )
    }

    pub fn use_state<T>(&mut self, initial: impl FnOnce() -> T) -> (T, StateSetter<T>)
    where
        T: Clone + Send + 'static,
    {
        let state = self.create_state_hook(self.scope.node_id(), initial);
        let value = state.get();
        (value, StateSetter::new(state))
    }

    pub fn use_state_eq<T>(&mut self, initial: impl FnOnce() -> T) -> (T, StateSetter<T>)
    where
        T: Clone + PartialEq + Send + 'static,
    {
        let (value, setter) = self.use_state(initial);
        (value, setter.with_equality())
    }

    pub fn state<T>(&mut self, initial: T) -> State<T>
    where
        T: Clone + Send + 'static,
    {
        self.state_with(|| initial)
    }

    pub fn state_with<T>(&mut self, initial: impl FnOnce() -> T) -> State<T>
    where
        T: Clone + Send + 'static,
    {
        self.create_state_hook(self.scope.node_id(), initial)
    }

    pub fn use_component_state<T, R>(&mut self, update: impl FnOnce(&mut T) -> R) -> R
    where
        T: ComponentState + Clone + Default + 'static,
    {
        let index = self.next_hook(HookKind::State).index();
        let id = self
            .scope
            .id(format!("h.{}.{index}", HookKind::State.code()));
        let (result, wants_frame) = self.context.component_state_mut_for_component(
            &id,
            self.component_id,
            self.scope.node_id(),
            |state: &mut T| {
                let result = update(state);
                (result, state.wants_frame())
            },
        );
        if wants_frame {
            self.context.hook_updates().request_frame();
        }
        result
    }

    fn create_state_hook<T>(&mut self, owner: UiId, initial: impl FnOnce() -> T) -> State<T>
    where
        T: Clone + Send + 'static,
    {
        let id = self.next_hook(HookKind::State);
        let value = self
            .context
            .hook_state(id, || Arc::new(Mutex::new(initial())));
        let updates = self.context.hook_updates();
        let component_id = self.component_id;
        State {
            value,
            invalidate: Arc::new(move || {
                updates.invalidate(component_id, owner.clone());
            }),
        }
    }

    pub fn use_effect<D, F, R>(&mut self, deps: D, effect: F)
    where
        D: Clone + PartialEq + 'static,
        F: FnOnce() -> R + 'static,
        R: super::IntoEffectCleanup,
    {
        let id = self.next_hook(HookKind::Effect);
        self.context.effect(id, deps, effect);
    }

    pub fn use_effect_once<F>(&mut self, effect: F)
    where
        F: FnOnce() + 'static,
    {
        self.use_effect((), effect);
    }

    #[cfg(feature = "async")]
    pub fn use_async_effect<D, F, Fut>(&mut self, deps: D, effect: F)
    where
        D: Clone + PartialEq + 'static,
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let spawner = self
            .context
            .task_spawner()
            .unwrap_or_else(super::noop_task_spawner);
        self.use_effect(deps, move || {
            let (cancel, task) = super::task::cancellable_task(Box::pin(effect()));
            spawner.spawn(task);
            move || cancel.cancel()
        });
    }

    #[cfg(feature = "async")]
    pub fn use_async_effect_once<F, Fut>(&mut self, effect: F)
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.use_async_effect((), effect);
    }

    pub fn use_mount<F>(&mut self, effect: F)
    where
        F: FnOnce() + 'static,
    {
        self.use_effect_once(effect);
    }

    pub fn use_context<T>(&mut self) -> T
    where
        T: Clone + 'static,
    {
        self.try_use_context::<T>().unwrap_or_else(|| {
            panic!(
                "missing context provider for `{}` in component {}",
                type_name::<T>(),
                self.component_id
            )
        })
    }

    pub fn try_use_context<T>(&mut self) -> Option<T>
    where
        T: Clone + 'static,
    {
        self.next_hook(HookKind::Context);
        self.context.contexts().read(self.component_id)
    }

    pub fn use_observable<T, S, F>(&mut self, source: Observable<T>, selector: F) -> S
    where
        T: Send + 'static,
        S: Clone + PartialEq + Send + 'static,
        F: Fn(&T) -> S + Copy + Send + Sync + 'static,
    {
        let id = self.next_hook(HookKind::Store);
        let current = selector(&source.read());
        let selected = self
            .context
            .hook_state(id, || Arc::new(Mutex::new(current.clone())));
        *selected.lock().expect("store selector state poisoned") = current.clone();

        let owner = self.component_id;
        let invalidation_id = self.scope.node_id();
        let updates = self.context.hook_updates();
        let source_id = source.id();
        self.use_effect((source_id, std::any::TypeId::of::<F>()), move || {
            let observed_source = source.clone();
            let observed_selected = Arc::clone(&selected);
            let listener = Arc::new(move || {
                let next = selector(&observed_source.read());
                let mut current = observed_selected
                    .lock()
                    .expect("store selector state poisoned");
                if *current == next {
                    return;
                }
                *current = next;
                updates.invalidate(owner, invalidation_id.clone());
            });
            let cleanup = source.subscribe(listener);
            move || cleanup()
        });
        current
    }

    #[cfg(feature = "router")]
    pub fn use_route<R>(&mut self) -> R
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().current().clone()
    }

    #[cfg(feature = "router")]
    pub fn use_navigate<R>(&mut self) -> Navigate<R>
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().navigate()
    }

    #[cfg(feature = "router")]
    pub fn use_replace<R>(&mut self) -> Replace<R>
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().replace()
    }

    #[cfg(feature = "router")]
    pub fn use_back<R>(&mut self) -> Back
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context::<RouterContext<R>>().back()
    }

    fn next_hook(&mut self, kind: HookKind) -> HookId {
        let index = self.hook_index;
        self.hook_index += 1;
        let kind = kind.slot_kind();
        self.context
            .component_tree()
            .record_hook(self.component_id, kind);
        HookId::new(self.component_id, index, kind)
    }
}

impl HookKind {
    fn code(self) -> u8 {
        match self {
            Self::Stable => 0,
            Self::State => 1,
            Self::Effect => 2,
            Self::Context => 3,
            Self::Store => 4,
        }
    }

    fn slot_kind(self) -> HookSlotKind {
        match self {
            Self::Stable => HookSlotKind::Stable,
            Self::State => HookSlotKind::State,
            Self::Effect => HookSlotKind::Effect,
            Self::Context => HookSlotKind::Context,
            Self::Store => HookSlotKind::Store,
        }
    }
}

impl Drop for RenderCx<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.context
                .component_tree()
                .abandon_component(self.component_id);
        } else {
            self.context
                .component_tree()
                .finish_component(self.component_id);
        }
    }
}

impl<T> Deref for StateSetter<T> {
    type Target = dyn Fn(T) + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        self.set.as_ref()
    }
}

impl<T> State<T>
where
    T: Clone + Send + 'static,
{
    pub fn get(&self) -> T {
        self.value.lock().expect("state value poisoned").clone()
    }

    pub fn set(&self, next: T) {
        *self.value.lock().expect("state value poisoned") = next;
        (self.invalidate)();
    }

    pub fn update(&self, update: impl FnOnce(&mut T)) {
        update(&mut self.value.lock().expect("state value poisoned"));
        (self.invalidate)();
    }

    pub fn try_update(&self, update: impl FnOnce(&mut T) -> bool) -> bool {
        let changed = update(&mut self.value.lock().expect("state value poisoned"));
        if changed {
            (self.invalidate)();
        }
        changed
    }
}

impl<T> StateSetter<T>
where
    T: Clone + Send + 'static,
{
    fn new(state: State<T>) -> Self {
        let set_state = state.clone();
        Self {
            set: Arc::new(move |next| set_state.set(next)),
            state,
        }
    }

    pub fn current(&self) -> T {
        self.state.get()
    }

    pub fn update(&self, update: impl FnOnce(&mut T) + Send + 'static) {
        self.state.update(update);
    }

    pub fn try_update(&self, update: impl FnOnce(&mut T) -> bool) -> bool {
        self.state.try_update(update)
    }

    fn with_equality(self) -> Self
    where
        T: PartialEq,
    {
        let current = self.state.clone();
        let set = Arc::clone(&self.set);
        Self {
            set: Arc::new(move |next| {
                if current.get() != next {
                    set(next);
                }
            }),
            state: self.state,
        }
    }
}

impl UiFocusHandle {
    pub fn focus(&self) {
        self.updates.request_focus(self.target.clone());
    }
}

#[cfg(test)]
#[path = "reactor/tests.rs"]
mod tests;
