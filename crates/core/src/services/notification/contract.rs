use std::{fmt, sync::Arc};

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
