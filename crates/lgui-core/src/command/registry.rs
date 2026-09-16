use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, RwLock},
};

use super::{Command, CommandContext, CommandHandler};

type ErasedValue = Box<dyn Any + Send>;
type ErasedResult = Result<ErasedValue, ErasedValue>;
type ErasedFuture = Pin<Box<dyn Future<Output = ErasedResult> + Send + 'static>>;

pub(super) trait ErasedCommandHandler: Send + Sync {
    fn invoke(&self, context: CommandContext, args: ErasedValue) -> ErasedFuture;
}

struct TypedCommandHandler<C, H> {
    handler: H,
    _command: PhantomData<fn() -> C>,
}

impl<C, H> ErasedCommandHandler for TypedCommandHandler<C, H>
where
    C: Command,
    H: CommandHandler<C>,
{
    fn invoke(&self, context: CommandContext, args: ErasedValue) -> ErasedFuture {
        let args = *args
            .downcast::<C::Args>()
            .unwrap_or_else(|_| panic!("command `{}` received an invalid argument type", C::NAME));
        let future = self.handler.call(context, args);
        Box::pin(async move {
            match future.await {
                Ok(output) => Ok(Box::new(output) as ErasedValue),
                Err(error) => Err(Box::new(error) as ErasedValue),
            }
        })
    }
}

#[derive(Clone, Default)]
pub(crate) struct CommandRegistry {
    handlers: Arc<RwLock<HashMap<TypeId, Arc<dyn ErasedCommandHandler>>>>,
}

impl CommandRegistry {
    pub(crate) fn register<C>(&self, handler: impl CommandHandler<C>) -> bool
    where
        C: Command,
    {
        let mut handlers = self.handlers.write().expect("command registry poisoned");
        if handlers.contains_key(&TypeId::of::<C>()) {
            return false;
        }
        handlers.insert(
            TypeId::of::<C>(),
            Arc::new(TypedCommandHandler::<C, _> {
                handler,
                _command: PhantomData,
            }),
        );
        true
    }

    pub(super) fn handler<C>(&self) -> Arc<dyn ErasedCommandHandler>
    where
        C: Command,
    {
        self.handlers
            .read()
            .expect("command registry poisoned")
            .get(&TypeId::of::<C>())
            .cloned()
            .unwrap_or_else(|| panic!("command `{}` is not registered", C::NAME))
    }
}
