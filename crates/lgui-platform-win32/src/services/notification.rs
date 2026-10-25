use std::io;

#[cfg(feature = "backend-winit")]
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};
use windows::{
    core::{HSTRING, PCWSTR},
    Data::Xml::Dom::{XmlDocument, XmlElement},
    UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
};

use lgui_services::{Notification, NotificationService};

#[cfg(feature = "backend-winit")]
use lgui_core::application::{Application, ApplicationContext};
#[cfg(feature = "backend-winit")]
use lgui_services::{NotificationError, NotificationHandle};

#[cfg(feature = "backend-winit")]
pub(crate) struct Win32NotificationRegistration {
    identity: String,
    display_name: Option<String>,
    icon: Option<std::path::PathBuf>,
}

#[cfg(feature = "backend-winit")]
pub trait Win32NotificationApplicationExt: Sized {
    /// Configures the built-in Windows toast notification adapter.
    ///
    /// The identity is only the toast source: it is registered for the current
    /// user but never becomes the process AppUserModelID, so the taskbar keeps
    /// using the window icon.
    fn notifications(self, identity: impl Into<String>) -> Self;

    /// Like [`Self::notifications`], with the name and icon file (`.ico` or
    /// `.png`) Windows shows on each toast.
    fn notifications_branded(
        self,
        identity: impl Into<String>,
        display_name: impl Into<String>,
        icon: impl Into<std::path::PathBuf>,
    ) -> Self;
}

#[cfg(feature = "backend-winit")]
impl<B> Win32NotificationApplicationExt for Application<B> {
    fn notifications(self, identity: impl Into<String>) -> Self {
        self.provide(Win32NotificationRegistration {
            identity: identity.into(),
            display_name: None,
            icon: None,
        })
    }

    fn notifications_branded(
        self,
        identity: impl Into<String>,
        display_name: impl Into<String>,
        icon: impl Into<std::path::PathBuf>,
    ) -> Self {
        self.provide(Win32NotificationRegistration {
            identity: identity.into(),
            display_name: Some(display_name.into()),
            icon: Some(icon.into()),
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

/// Registers the toast source under `HKCU\Software\Classes\AppUserModelId`.
///
/// Deliberately not `SetCurrentProcessExplicitAppUserModelID`: once a process
/// sets one, the shell resolves the taskbar icon from that ID's registration
/// instead of the window, which shows the generic executable icon.
#[cfg(feature = "backend-winit")]
fn register_toast_source(registration: &Win32NotificationRegistration) -> io::Result<()> {
    let subkey = HSTRING::from(format!(
        r"Software\Classes\AppUserModelId\{}",
        registration.identity
    ));
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &subkey,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    }
    .ok()
    .map_err(windows_error)?;
    let display_name = registration
        .display_name
        .as_deref()
        .unwrap_or(&registration.identity);
    let mut result = set_registry_string(key, "DisplayName", display_name);
    if result.is_ok()
        && let Some(icon) = &registration.icon
    {
        result = set_registry_string(key, "IconUri", &icon.to_string_lossy());
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

#[cfg(feature = "backend-winit")]
fn set_registry_string(key: HKEY, name: &str, value: &str) -> io::Result<()> {
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    unsafe { RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(bytes)) }
        .ok()
        .map_err(windows_error)
}

#[cfg(feature = "backend-winit")]
#[doc(hidden)]
pub fn install_notification_service(context: &ApplicationContext) -> io::Result<()> {
    let Some(registration) = context.try_resource::<Win32NotificationRegistration>() else {
        return Ok(());
    };
    register_toast_source(&registration)?;
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
