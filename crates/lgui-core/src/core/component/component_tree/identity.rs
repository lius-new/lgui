use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId {
    pub(super) index: u32,
    pub(super) generation: u32,
}

impl ComponentId {
    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.index, self.generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HookSlotKind {
    Stable,
    State,
    Effect,
    Listener,
    Context,
    Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HookId {
    component: ComponentId,
    index: u32,
    kind: HookSlotKind,
}

impl HookId {
    pub const fn new(component: ComponentId, index: usize, kind: HookSlotKind) -> Self {
        Self {
            component,
            index: index as u32,
            kind,
        }
    }

    pub const fn component(self) -> ComponentId {
        self.component
    }

    pub const fn index(self) -> usize {
        self.index as usize
    }

    pub const fn kind(self) -> HookSlotKind {
        self.kind
    }
}

impl fmt::Display for HookId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "component {} hook {} ({:?})",
            self.component, self.index, self.kind
        )
    }
}
