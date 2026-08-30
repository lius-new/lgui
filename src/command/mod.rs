use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, RwLock},
};

use crate::{application::ApplicationContext, events::Event};

type ErasedValue = Box<dyn Any + Send>;
type ErasedResult = Result<ErasedValue, ErasedValue>;
type ErasedFuture = Pin<Box<dyn Future<Output = ErasedResult> + Send + 'static>>;

pub type CommandFuture<C> = Pin<
    Box<
        dyn Future<Output = Result<<C as Command>::Output, <C as Command>::Error>> + Send + 'static,
    >,
>;

/// A typed application command contract.
///
/// `NAME` is used only for diagnostics. Commands are registered and resolved by
/// their Rust type, so arguments and results never pass through serialization.
pub trait Command: Send + Sync + 'static {
    type Args: Send + 'static;
    type Output: Send + 'static;
    type Error: Send + 'static;

    const NAME: &'static str;
}

pub trait CommandHandler<C>: Send + Sync + 'static
where
    C: Command,
{
    fn call(&self, context: CommandContext, args: C::Args) -> CommandFuture<C>;
}

impl<C, F, Fut> CommandHandler<C> for F
where
    C: Command,
    F: Fn(CommandContext, C::Args) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<C::Output, C::Error>> + Send + 'static,
{
    fn call(&self, context: CommandContext, args: C::Args) -> CommandFuture<C> {
        Box::pin((self)(context, args))
    }
}

#[derive(Clone)]
pub struct CommandContext {
    application: ApplicationContext,
}

impl CommandContext {
    pub(crate) fn new(application: ApplicationContext) -> Self {
        Self { application }
    }

    pub fn application(&self) -> ApplicationContext {
        self.application.clone()
    }

    pub fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.application.resource::<T>()
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.application.emit(event)
    }
}

trait ErasedCommandHandler: Send + Sync {
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

    fn handler<C>(&self) -> Arc<dyn ErasedCommandHandler>
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

pub struct CommandHandle<C> {
    application: ApplicationContext,
    _command: PhantomData<fn() -> C>,
}

impl<C> Clone for CommandHandle<C> {
    fn clone(&self) -> Self {
        Self {
            application: self.application.clone(),
            _command: PhantomData,
        }
    }
}

impl<C> CommandHandle<C>
where
    C: Command,
{
    pub(crate) fn new(application: ApplicationContext) -> Self {
        Self {
            application,
            _command: PhantomData,
        }
    }

    pub async fn invoke(&self, args: C::Args) -> Result<C::Output, C::Error> {
        let handler = self.application.command_registry().handler::<C>();
        let result = handler
            .invoke(
                CommandContext::new(self.application.clone()),
                Box::new(args),
            )
            .await;
        match result {
            Ok(output) => Ok(output
                .downcast::<C::Output>()
                .map(|output| *output)
                .unwrap_or_else(|_| {
                    panic!("command `{}` returned an invalid output type", C::NAME)
                })),
            Err(error) => Err(*error.downcast::<C::Error>().unwrap_or_else(|_| {
                panic!("command `{}` returned an invalid error type", C::NAME)
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};

    use super::*;

    struct Add;

    impl Command for Add {
        type Args = (i32, i32);
        type Output = i32;
        type Error = &'static str;

        const NAME: &'static str = "test.add";
    }

    fn run_ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly pending"),
        }
    }

    #[test]
    fn typed_commands_preserve_arguments_outputs_and_errors() {
        let application = ApplicationContext::empty();
        assert!(application
            .command_registry()
            .register::<Add>(|_, (left, right)| async move {
                if left < 0 {
                    Err("negative")
                } else {
                    Ok(left + right)
                }
            }));

        let command = application.command::<Add>();
        assert_eq!(run_ready(command.invoke((2, 3))), Ok(5));
        assert_eq!(run_ready(command.invoke((-1, 3))), Err("negative"));
    }

    #[test]
    fn duplicate_command_registration_is_rejected() {
        let application = ApplicationContext::empty();
        assert!(application
            .command_registry()
            .register::<Add>(|_, _| async { Ok(0) }));
        assert!(!application
            .command_registry()
            .register::<Add>(|_, _| async { Ok(1) }));
    }
}
