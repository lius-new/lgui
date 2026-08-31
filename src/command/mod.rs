//! Typed, application-scoped commands.

mod context;
mod contract;
mod handle;
mod registry;

pub use context::CommandContext;
pub use contract::{Command, CommandFuture, CommandHandler};
pub use handle::CommandHandle;
pub(crate) use registry::CommandRegistry;

#[cfg(test)]
mod tests;
