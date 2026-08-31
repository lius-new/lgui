use super::{TrayMenuEntry, TrayMenuItem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayAction {
    ShowMainWindow,
    HideMainWindow,
    Exit,
    Command {
        name: String,
        show_main_window: bool,
    },
}

impl TrayAction {
    pub fn command(command: impl Into<String>) -> Self {
        Self::Command {
            name: command.into(),
            show_main_window: false,
        }
    }

    pub fn command_and_show_main(command: impl Into<String>) -> Self {
        Self::Command {
            name: command.into(),
            show_main_window: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayOptions {
    pub(crate) tooltip: String,
    pub(crate) icon_bytes: Option<&'static [u8]>,
    pub(crate) items: Vec<TrayMenuEntry<TrayAction>>,
    pub(crate) activate: Option<TrayAction>,
}

impl TrayOptions {
    pub fn new(tooltip: impl Into<String>) -> Self {
        Self {
            tooltip: tooltip.into(),
            icon_bytes: None,
            items: Vec::new(),
            activate: None,
        }
    }

    pub fn icon_bytes(mut self, bytes: &'static [u8]) -> Self {
        self.icon_bytes = Some(bytes);
        self
    }

    pub fn item(mut self, item: TrayMenuItem<TrayAction>) -> Self {
        self.items.push(item.into());
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(TrayMenuEntry::Separator);
        self
    }

    pub fn activate(mut self, action: TrayAction) -> Self {
        self.activate = Some(action);
        self
    }
}
