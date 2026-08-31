use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use crate::{
    core::{Size, UiEventContext, UiRect},
    platform::dpi::ScalePreference,
};

use super::WindowId;

pub type WindowDragExclusion = fn(f32, f32) -> UiRect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowPosition {
    #[default]
    Centered,
    AdjacentToOwner {
        gap: i32,
    },
    NearCursor {
        gap: i32,
    },
    Absolute {
        x: i32,
        y: i32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMode {
    #[default]
    Windowed,
    Fullscreen,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClosePolicy {
    #[default]
    Exit,
    Hide,
    Notify,
}

pub type WindowCloseHandler = fn(&mut UiEventContext);

#[derive(Clone, Default)]
struct WindowOptionExtensions {
    values: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for WindowOptionExtensions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowOptionExtensions")
            .field("count", &self.values.len())
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub id: WindowId,
    pub owner: Option<WindowId>,
    pub title: String,
    pub visible: bool,
    pub size: Size,
    pub minimum_size: Option<Size>,
    pub maximum_size: Option<Size>,
    pub resizable: bool,
    pub native_titlebar: bool,
    pub position: WindowPosition,
    pub transparent: bool,
    pub corner_radius: i32,
    pub topmost: bool,
    pub hide_on_deactivate: bool,
    pub background_memory_optimization: bool,
    /// COMPATIBILITY: remove after consumers migrate to `Element::window_drag_region`.
    pub titlebar_drag_height: Option<f32>,
    /// COMPATIBILITY: remove after consumers migrate to `Element::window_drag_region`.
    pub drag_exclusion: Option<WindowDragExclusion>,
    pub scale_reference_size: Option<Size>,
    pub scale_preference: ScalePreference,
    pub mode: WindowMode,
    pub close_policy: ClosePolicy,
    pub close_handler: Option<WindowCloseHandler>,
    extensions: WindowOptionExtensions,
}

impl WindowOptions {
    pub fn new(id: impl Into<WindowId>) -> Self {
        let id = id.into();
        Self {
            title: id.as_str().to_owned(),
            id,
            ..Self::default()
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn owner(mut self, owner: impl Into<WindowId>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn minimum_size(mut self, size: Size) -> Self {
        self.minimum_size = Some(size);
        self
    }

    pub fn maximum_size(mut self, size: Size) -> Self {
        self.maximum_size = Some(size);
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn native_titlebar(mut self, enabled: bool) -> Self {
        self.native_titlebar = enabled;
        self
    }

    pub fn position(mut self, position: WindowPosition) -> Self {
        self.position = position;
        self
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn corner_radius(mut self, radius: i32) -> Self {
        self.corner_radius = radius.max(0);
        self
    }

    pub fn topmost(mut self, topmost: bool) -> Self {
        self.topmost = topmost;
        self
    }

    pub fn hide_on_deactivate(mut self, hide: bool) -> Self {
        self.hide_on_deactivate = hide;
        self
    }

    /// Releases reconstructible render state while hidden. When all top-level windows are
    /// hidden, shared caches and the process working set are also trimmed. Component state,
    /// effects and background tasks remain alive.
    pub fn background_memory_optimization(mut self, enabled: bool) -> Self {
        self.background_memory_optimization = enabled;
        self
    }

    #[deprecated(
        note = "geometry-based titlebar drag is a compatibility path; migrate immediately to Element::window_drag_region"
    )]
    pub fn titlebar_drag(mut self, height: f32, exclusion: Option<WindowDragExclusion>) -> Self {
        self.titlebar_drag_height = Some(height.max(0.0));
        self.drag_exclusion = exclusion;
        self
    }

    pub fn scale_reference_size(mut self, size: Size) -> Self {
        self.scale_reference_size = Some(size);
        self
    }

    pub fn scale_preference(mut self, preference: ScalePreference) -> Self {
        self.scale_preference = preference;
        self
    }

    pub fn mode(mut self, mode: WindowMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn close_policy(mut self, policy: ClosePolicy) -> Self {
        self.close_policy = policy;
        if policy != ClosePolicy::Notify {
            self.close_handler = None;
        }
        self
    }

    pub fn on_close_requested(mut self, handler: WindowCloseHandler) -> Self {
        self.close_policy = ClosePolicy::Notify;
        self.close_handler = Some(handler);
        self
    }

    pub fn with_platform_options<T>(mut self, options: T) -> Self
    where
        T: Any + Send + Sync,
    {
        self.extensions
            .values
            .insert(TypeId::of::<T>(), Arc::new(options));
        self
    }

    pub fn platform_options<T>(&self) -> Option<&T>
    where
        T: Any + Send + Sync,
    {
        self.extensions
            .values
            .get(&TypeId::of::<T>())
            .and_then(|options| options.downcast_ref())
    }
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            id: WindowId::new("main"),
            owner: None,
            title: "lgui".to_owned(),
            visible: true,
            size: Size::new(1024.0, 720.0),
            minimum_size: None,
            maximum_size: None,
            resizable: true,
            native_titlebar: true,
            position: WindowPosition::Centered,
            transparent: false,
            corner_radius: 0,
            topmost: false,
            hide_on_deactivate: false,
            background_memory_optimization: false,
            titlebar_drag_height: None,
            drag_exclusion: None,
            scale_reference_size: None,
            scale_preference: ScalePreference::Auto,
            mode: WindowMode::Windowed,
            close_policy: ClosePolicy::Exit,
            close_handler: None,
            extensions: WindowOptionExtensions::default(),
        }
    }
}
