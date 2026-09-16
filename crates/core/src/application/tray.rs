use std::sync::Arc;

use crate::services::{TrayAction, TrayOptions};

use super::ApplicationContext;

pub type TrayCommandHandler = Arc<dyn Fn(&ApplicationContext, &str) + Send + Sync + 'static>;

#[doc(hidden)]
pub struct TrayRegistration {
    pub options: TrayOptions,
    pub handler: TrayCommandHandler,
}

#[doc(hidden)]
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

#[cfg(test)]
#[path = "tray_test.rs"]
mod tests;
