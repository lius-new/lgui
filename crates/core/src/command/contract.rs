use std::{future::Future, pin::Pin};

use super::CommandContext;

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
