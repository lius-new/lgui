//! Typed, application-scoped commands.

mod context;
mod contract;
mod handle;
mod registry;

pub use context::CommandContext;
pub use contract::{Command, CommandFuture, CommandHandler};
pub use handle::CommandHandle;
pub(crate) use registry::CommandRegistry;

/// Invokes a typed Command in the active Application.
///
/// LGUI async handlers and tasks automatically carry their Application scope.
/// Futures submitted to an external executor must first be wrapped with
/// `ApplicationContext::scope`.
pub async fn invoke<C>(args: C::Args) -> Result<C::Output, C::Error>
where
    C: Command,
{
    let application = crate::application::current_application("invoke");
    application.command::<C>().invoke(args).await
}

#[cfg(test)]
mod tests;
