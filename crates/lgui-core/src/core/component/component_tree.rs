use super::{UiElement, UiId};

mod identity;
mod lifecycle;
mod memory;
mod storage;

pub use identity::{ComponentId, HookId, HookSlotKind};
pub use storage::{ComponentRuntimeMetrics, ComponentTree};

#[cfg(test)]
#[path = "component_tree_test.rs"]
mod tests;
