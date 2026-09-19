#[cfg(any(
    feature = "clipboard",
    feature = "dialogs",
    feature = "notifications",
    feature = "tray"
))]
use std::sync::Arc;

use lgui_core::application::{Application, ApplicationContext};

#[cfg(feature = "tray")]
use crate::{TrayAction, TrayOptions};

#[cfg(feature = "tray")]
pub type TrayCommandHandler = Arc<dyn Fn(&ApplicationContext, &str) + Send + Sync + 'static>;

#[doc(hidden)]
#[cfg(feature = "tray")]
pub struct TrayRegistration {
    pub options: TrayOptions,
    pub handler: TrayCommandHandler,
}

#[doc(hidden)]
#[cfg(feature = "tray")]
pub fn dispatch_tray_action(
    registration: &TrayRegistration,
    context: &ApplicationContext,
    action: TrayAction,
    set_main_window_visibility: impl FnOnce(bool),
) {
    match action {
        TrayAction::ShowMainWindow => set_main_window_visibility(true),
        TrayAction::HideMainWindow => set_main_window_visibility(false),
        TrayAction::Exit => {
            context.windows().exit();
        }
        TrayAction::Command {
            name,
            show_main_window,
        } => {
            (registration.handler)(context, &name);
            if show_main_window {
                set_main_window_visibility(true);
            }
        }
    }
}

pub trait ServicesApplicationExt: Sized {
    #[cfg(feature = "notifications")]
    fn notification_service(self, service: crate::NotificationHandle) -> Self;

    #[cfg(feature = "tray")]
    fn tray(
        self,
        options: TrayOptions,
        handler: impl Fn(&ApplicationContext, &str) + Send + Sync + 'static,
    ) -> Self;
}

impl<B> ServicesApplicationExt for Application<B> {
    #[cfg(feature = "notifications")]
    fn notification_service(self, service: crate::NotificationHandle) -> Self {
        self.provide(service)
    }

    #[cfg(feature = "tray")]
    fn tray(
        self,
        options: TrayOptions,
        handler: impl Fn(&ApplicationContext, &str) + Send + Sync + 'static,
    ) -> Self {
        self.provide(TrayRegistration {
            options,
            handler: Arc::new(handler),
        })
    }
}

pub trait ServicesContextExt {
    #[cfg(feature = "notifications")]
    fn notifications(&self) -> Option<Arc<crate::NotificationHandle>>;

    #[cfg(feature = "clipboard")]
    fn clipboard(&self) -> crate::ClipboardHandle;

    #[cfg(feature = "open-url")]
    fn open_url(&self, url: &str) -> Result<(), crate::open_url::OpenUrlError>;

    #[cfg(feature = "dialogs")]
    fn file_dialogs(&self) -> Arc<crate::dialogs::FileDialogHandle>;
}

impl ServicesContextExt for ApplicationContext {
    #[cfg(feature = "notifications")]
    fn notifications(&self) -> Option<Arc<crate::NotificationHandle>> {
        self.try_resource::<crate::NotificationHandle>()
    }

    #[cfg(feature = "clipboard")]
    fn clipboard(&self) -> crate::ClipboardHandle {
        self.try_resource::<crate::ClipboardHandle>()
            .map(|clipboard| (*clipboard).clone())
            .unwrap_or_else(crate::clipboard::system_clipboard)
    }

    #[cfg(feature = "open-url")]
    fn open_url(&self, url: &str) -> Result<(), crate::open_url::OpenUrlError> {
        self.try_resource::<crate::open_url::OpenUrlHandle>()
            .map(|opener| opener.open(url))
            .unwrap_or_else(|| crate::open_url::system_url_opener().open(url))
    }

    #[cfg(feature = "dialogs")]
    fn file_dialogs(&self) -> Arc<crate::dialogs::FileDialogHandle> {
        self.try_resource::<crate::dialogs::FileDialogHandle>()
            .unwrap_or_else(|| Arc::new(crate::dialogs::system_file_dialogs()))
    }
}

#[cfg(test)]
#[cfg(feature = "tray")]
#[path = "application_test.rs"]
mod tests;
