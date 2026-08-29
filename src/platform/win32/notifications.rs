use std::io;

use windows::{
    core::HSTRING,
    Data::Xml::Dom::{XmlDocument, XmlElement},
    Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID,
    UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
};

use crate::platform::{Notification, NotificationService};

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

pub(crate) fn initialize_process_identity(app_user_model_id: &str) -> io::Result<()> {
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_user_model_id)) }
        .map_err(windows_error)
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
