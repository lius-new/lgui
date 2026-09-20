use std::sync::{Arc, RwLock};

use crate::{
    core::{Element, InputEvent, RenderCx},
    platform::dpi::ScalePreference,
};

use super::{WindowCommand, WindowId, WindowMode, WindowOptions};

type WindowCommandHandler = Arc<dyn Fn(WindowCommand) + Send + Sync>;

#[derive(Clone)]
pub struct WindowManager {
    inner: Arc<WindowManagerInner>,
}

struct WindowManagerInner {
    command: RwLock<Option<WindowCommandHandler>>,
}

impl WindowManager {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(WindowManagerInner {
                command: RwLock::new(None),
            }),
        }
    }

    pub(crate) fn install(&self, command: impl Fn(WindowCommand) + Send + Sync + 'static) {
        *self.inner.command.write().expect("window manager poisoned") = Some(Arc::new(command));
    }

    pub fn show(
        &self,
        options: WindowOptions,
        view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
    ) -> bool {
        self.send(WindowCommand::Show {
            options,
            view: Arc::new(view),
        })
    }

    pub fn hide(&self, id: impl Into<WindowId>) -> bool {
        self.send(WindowCommand::Hide(id.into()))
    }

    pub fn toggle(
        &self,
        options: WindowOptions,
        view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
    ) -> bool {
        self.send(WindowCommand::Toggle {
            options,
            view: Arc::new(view),
        })
    }

    pub fn close(&self, id: impl Into<WindowId>) -> bool {
        self.send(WindowCommand::Close(id.into()))
    }

    pub fn request_close(&self, id: impl Into<WindowId>) -> bool {
        self.send(WindowCommand::RequestClose(id.into()))
    }

    pub fn minimize(&self, id: impl Into<WindowId>) -> bool {
        self.send(WindowCommand::Minimize(id.into()))
    }

    pub fn set_scale_preference(&self, preference: ScalePreference) -> bool {
        self.send(WindowCommand::SetScalePreference(preference))
    }

    pub fn set_mode(&self, id: impl Into<WindowId>, mode: WindowMode) -> bool {
        self.send(WindowCommand::SetMode {
            id: id.into(),
            mode,
        })
    }

    pub fn toggle_maximize(&self, id: impl Into<WindowId>) -> bool {
        self.send(WindowCommand::ToggleMaximize(id.into()))
    }

    pub fn send_input(&self, id: impl Into<WindowId>, input: InputEvent) -> bool {
        self.send(WindowCommand::Input {
            id: id.into(),
            input,
        })
    }

    pub fn exit(&self) -> bool {
        self.send(WindowCommand::Exit)
    }

    fn send(&self, command: WindowCommand) -> bool {
        let handler = self
            .inner
            .command
            .read()
            .expect("window manager poisoned")
            .clone();
        let Some(handler) = handler else {
            return false;
        };
        handler(command);
        true
    }
}

impl PartialEq for WindowManager {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

#[derive(Clone)]
pub struct WindowHandle {
    id: WindowId,
    windows: WindowManager,
}

impl WindowHandle {
    pub(crate) fn new(id: WindowId, windows: WindowManager) -> Self {
        Self { id, windows }
    }

    pub fn id(&self) -> &WindowId {
        &self.id
    }

    pub fn close(&self) -> bool {
        self.windows.close(self.id.clone())
    }

    pub fn request_close(&self) -> bool {
        self.windows.request_close(self.id.clone())
    }

    pub fn minimize(&self) -> bool {
        self.windows.minimize(self.id.clone())
    }

    pub fn hide(&self) -> bool {
        self.windows.hide(self.id.clone())
    }

    pub fn set_mode(&self, mode: WindowMode) -> bool {
        self.windows.set_mode(self.id.clone(), mode)
    }

    pub fn toggle_maximize(&self) -> bool {
        self.windows.toggle_maximize(self.id.clone())
    }
}
