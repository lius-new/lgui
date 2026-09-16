use std::io;

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::{
    core::HSTRING,
    Data::Xml::Dom::{XmlDocument, XmlElement},
    UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
};

use lgui_services::{Notification, NotificationService};

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
use lgui_core::application::{Application, ApplicationContext};
#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
use lgui_services::{NotificationError, NotificationHandle};

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
pub(crate) struct Win32NotificationRegistration {
    identity: String,
}

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
pub trait Win32NotificationApplicationExt: Sized {
    /// Configures the built-in Windows toast notification adapter.
    fn notifications(self, identity: impl Into<String>) -> Self;
}

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
impl<B, M> Win32NotificationApplicationExt for Application<B, M> {
    fn notifications(self, identity: impl Into<String>) -> Self {
        self.provide(Win32NotificationRegistration {
            identity: identity.into(),
        })
    }
}

pub struct Win32NotificationService {
    app_user_model_id: String,
}

impl Win32NotificationService {
    pub fn new(app_user_model_id: impl Into<String>) -> io::Result<Self> {
        Ok(Self {
            app_user_model_id: app_user_model_id.into(),
        })
    }

    pub fn show(&self, title: &str, body: &str) -> io::Result<()> {
        <Self as NotificationService>::show(self, &Notification::new(title, body))
    }
}

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
fn initialize_process_identity(app_user_model_id: &str) -> io::Result<()> {
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_user_model_id)) }
        .map_err(windows_error)
}

#[cfg(any(feature = "backend-win32", feature = "backend-winit"))]
#[doc(hidden)]
pub fn install_notification_service(context: &ApplicationContext) -> io::Result<()> {
    let Some(registration) = context.try_resource::<Win32NotificationRegistration>() else {
        return Ok(());
    };
    initialize_process_identity(&registration.identity)?;
    if context.try_resource::<NotificationHandle>().is_some() {
        return Ok(());
    }
    let service = Win32NotificationService::new(&registration.identity)?;
    context
        .resources()
        .provide(NotificationHandle::new(move |notification| {
            service
                .show(&notification.title, &notification.body)
                .map_err(|error| NotificationError::new(error.to_string()))
        }));
    Ok(())
}

impl NotificationService for Win32NotificationService {
    type Error = io::Error;

    fn show(&self, notification: &Notification) -> Result<(), Self::Error> {
        let document =
            build_document(&notification.title, &notification.body).map_err(windows_error)?;
        let notification =
            ToastNotification::CreateToastNotification(&document).map_err(windows_error)?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
            &self.app_user_model_id,
        ))
        .map_err(windows_error)?;
        let setting = notifier.Setting().map_err(windows_error)?;
        if setting != NotificationSetting::Enabled {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Windows notification setting is disabled ({})", setting.0),
            ));
        }
        notifier.Show(&notification).map_err(windows_error)
    }
}

fn build_document(title: &str, body: &str) -> windows::core::Result<XmlDocument> {
    let document = XmlDocument::new()?;
    let toast = document.CreateElement(&HSTRING::from("toast"))?;
    document.AppendChild(&toast)?;
    let visual = append_element(&document, &toast, "visual")?;
    let binding = append_element(&document, &visual, "binding")?;
    binding.SetAttribute(&HSTRING::from("template"), &HSTRING::from("ToastGeneric"))?;
    append_text(&document, &binding, title)?;
    append_text(&document, &binding, body)?;
    Ok(document)
}

fn append_element(
    document: &XmlDocument,
    parent: &XmlElement,
    name: &str,
) -> windows::core::Result<XmlElement> {
    let element = document.CreateElement(&HSTRING::from(name))?;
    parent.AppendChild(&element)?;
    Ok(element)
}

fn append_text(
    document: &XmlDocument,
    parent: &XmlElement,
    value: &str,
) -> windows::core::Result<()> {
    let element = append_element(document, parent, "text")?;
    let text = document.CreateTextNode(&HSTRING::from(value))?;
    element.AppendChild(&text)?;
    Ok(())
}

fn windows_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}
