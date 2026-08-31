use std::sync::Arc;

use crate::services::{TrayAction, TrayOptions};

use super::ApplicationContext;

pub(crate) type TrayCommandHandler = Arc<dyn Fn(&ApplicationContext, &str) + Send + Sync + 'static>;

pub(crate) struct TrayRegistration {
    pub options: TrayOptions,
    pub handler: TrayCommandHandler,
}

pub(crate) fn dispatch_tray_action(
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
#[path = "tray/tests.rs"]
mod tests;
