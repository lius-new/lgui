use std::{fmt, sync::Arc};

use crate::{
    core::{InputEvent, RuntimeOutput, UiTask, UiTaskSpawner, UiWake},
    session::UiSession,
};

pub mod dpi;

#[cfg(feature = "renderer-skia")]
#[allow(unsafe_code)]
pub(crate) mod skia;

#[cfg(all(feature = "backend-win32", target_os = "windows"))]
#[allow(unsafe_code)]
pub mod win32;

#[cfg(feature = "backend-winit")]
mod winit;
#[cfg(feature = "renderer-skia-gl")]
#[allow(unsafe_code)]
mod winit_skia_gl;
#[cfg(all(feature = "backend-winit", target_os = "windows"))]
mod winit_windows;
#[cfg(feature = "accessibility")]
mod winit_accessibility;

#[cfg(feature = "backend-winit")]
pub use winit::{WinitApplication, WinitApplicationError};

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

#[derive(Clone)]
pub struct WakeHandle {
    wake: UiWake,
}

impl WakeHandle {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            wake: Arc::new(wake),
        }
    }

    pub fn wake(&self) {
        (self.wake)();
    }

    pub fn as_ui_wake(&self) -> UiWake {
        Arc::clone(&self.wake)
    }
}

pub trait InputSink {
    fn handle_input(&mut self, input: InputEvent) -> RuntimeOutput;
}

impl InputSink for UiSession {
    fn handle_input(&mut self, input: InputEvent) -> RuntimeOutput {
        UiSession::handle_input(self, input)
    }
}

pub fn task_spawner(executor: impl Fn(UiTask) + Send + Sync + 'static) -> UiTaskSpawner {
    Arc::new(executor)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn wake_handle_is_cloneable_and_backend_neutral() {
        let count = Arc::new(AtomicUsize::new(0));
        let wake = WakeHandle::new({
            let count = Arc::clone(&count);
            move || {
                count.fetch_add(1, Ordering::Relaxed);
            }
        });

        wake.clone().wake();
        (wake.as_ui_wake())();

        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
