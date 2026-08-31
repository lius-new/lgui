use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable,
    AccessDenied,
    AllocationFailed,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "clipboard text is unavailable",
            Self::AccessDenied => "clipboard access was denied",
            Self::AllocationFailed => "clipboard allocation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ClipboardError {}

pub trait Clipboard: Send + Sync + 'static {
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;
}

pub type ClipboardHandle = Arc<dyn Clipboard>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

impl Notification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
        }
    }
}

pub trait NotificationService: Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn show(&self, notification: &Notification) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationError(String);

impl NotificationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NotificationError {}

#[derive(Clone)]
pub struct NotificationHandle {
    show: Arc<dyn Fn(&Notification) -> Result<(), NotificationError> + Send + Sync>,
}

impl NotificationHandle {
    pub fn new(
        show: impl Fn(&Notification) -> Result<(), NotificationError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            show: Arc::new(show),
        }
    }

    pub fn show(&self, notification: &Notification) -> Result<(), NotificationError> {
        (self.show)(notification)
    }
}

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
