#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuItem<Command> {
    pub label: String,
    pub icon: Option<&'static str>,
    pub command: Command,
    pub enabled: bool,
    pub checked: bool,
}

impl<Command> TrayMenuItem<Command> {
    pub fn new(label: impl Into<String>, command: Command) -> Self {
        Self {
            label: label.into(),
            icon: None,
            command,
            enabled: true,
            checked: false,
        }
    }

    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayMenuEntry<Command> {
    Item(TrayMenuItem<Command>),
    Separator,
}

impl<Command> From<TrayMenuItem<Command>> for TrayMenuEntry<Command> {
    fn from(item: TrayMenuItem<Command>) -> Self {
        Self::Item(item)
    }
}

pub trait TrayService<Command>: Send + Sync + 'static
where
    Command: Clone + Send + Sync + 'static,
{
    type Error: std::error::Error + Send + Sync + 'static;

    fn install(&self, tooltip: &str, menu: &[TrayMenuEntry<Command>]) -> Result<(), Self::Error>;
    fn update_menu(&self, menu: &[TrayMenuEntry<Command>]) -> Result<(), Self::Error>;
    fn remove(&self);
}
