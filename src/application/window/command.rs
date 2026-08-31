use crate::{core::InputEvent, platform::dpi::ScalePreference};

use super::{WindowId, WindowMode, WindowOptions};
use crate::application::AppView;

pub(crate) enum WindowCommand {
    Show {
        options: WindowOptions,
        view: AppView,
    },
    Hide(WindowId),
    Toggle {
        options: WindowOptions,
        view: AppView,
    },
    Close(WindowId),
    RequestClose(WindowId),
    Minimize(WindowId),
    SetScalePreference(ScalePreference),
    SetMode {
        id: WindowId,
        mode: WindowMode,
    },
    Input {
        id: WindowId,
        input: InputEvent,
    },
    Exit,
}
