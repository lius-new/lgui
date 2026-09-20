use crate::{core::InputEvent, platform::dpi::ScalePreference};

use super::{WindowId, WindowMode, WindowOptions, WindowView};

#[doc(hidden)]
pub enum WindowCommand {
    Show {
        options: WindowOptions,
        view: WindowView,
    },
    Hide(WindowId),
    Toggle {
        options: WindowOptions,
        view: WindowView,
    },
    Close(WindowId),
    RequestClose(WindowId),
    Minimize(WindowId),
    SetScalePreference(ScalePreference),
    SetMode {
        id: WindowId,
        mode: WindowMode,
    },
    ToggleMaximize(WindowId),
    Input {
        id: WindowId,
        input: InputEvent,
    },
    Exit,
}
