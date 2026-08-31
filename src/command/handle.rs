use std::marker::PhantomData;

use crate::application::ApplicationContext;

use super::{Command, CommandContext};

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
