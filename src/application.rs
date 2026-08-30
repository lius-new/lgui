use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    sync::{Arc, RwLock},
};

#[cfg(feature = "router")]
use std::sync::Mutex;

use crate::platform::dpi::ScalePreference;
#[cfg(feature = "notifications")]
use crate::platform::NotificationHandle;
#[cfg(feature = "tray")]
use crate::platform::{TrayMenuEntry, TrayMenuItem};
use crate::{
    command::{Command, CommandHandle, CommandHandler, CommandRegistry},
    core::{
        component, context_provider, Element, RenderCx, RootComponent, Size, UiExecutor, UiRect,
        UiTaskSpawner,
    },
    events::{Event, EventBus, EventSubscription},
    resources::Resources,
};

pub use crate::renderer::RenderErrorStage;

#[cfg(feature = "router")]
use crate::router::Router;
#[cfg(feature = "store")]
use crate::store::{StoreContext, StoreRuntime};

pub type AppView = Arc<
    dyn for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
>;

pub type ApplicationTask = Box<dyn FnOnce() + Send + 'static>;

type WindowCommandHandler = Arc<dyn Fn(WindowCommand) + Send + Sync>;

type RenderErrorHandler = Arc<dyn Fn(&RenderError) + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct RenderErrorRegistration {
    handler: RenderErrorHandler,
}

#[cfg(feature = "tray")]
pub(crate) type TrayCommandHandler = Arc<dyn Fn(&ApplicationContext, &str) + Send + Sync + 'static>;

#[cfg(feature = "tray")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayAction {
    ShowMainWindow,
    HideMainWindow,
    Exit,
    Command {
        name: String,
        show_main_window: bool,
    },
}

#[cfg(feature = "tray")]
impl TrayAction {
    pub fn command(command: impl Into<String>) -> Self {
        Self::Command {
            name: command.into(),
            show_main_window: false,
        }
    }

    pub fn command_and_show_main(command: impl Into<String>) -> Self {
        Self::Command {
            name: command.into(),
            show_main_window: true,
        }
    }
}

#[cfg(feature = "tray")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayOptions {
    pub(crate) tooltip: String,
    pub(crate) icon_bytes: Option<&'static [u8]>,
    pub(crate) items: Vec<TrayMenuEntry<TrayAction>>,
    pub(crate) activate: Option<TrayAction>,
}

#[cfg(feature = "tray")]
impl TrayOptions {
    pub fn new(tooltip: impl Into<String>) -> Self {
        Self {
            tooltip: tooltip.into(),
            icon_bytes: None,
            items: Vec::new(),
            activate: None,
        }
    }

    pub fn icon_bytes(mut self, bytes: &'static [u8]) -> Self {
        self.icon_bytes = Some(bytes);
        self
    }

    pub fn item(mut self, item: TrayMenuItem<TrayAction>) -> Self {
        self.items.push(item.into());
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(TrayMenuEntry::Separator);
        self
    }

    pub fn activate(mut self, action: TrayAction) -> Self {
        self.activate = Some(action);
        self
    }
}

#[cfg(feature = "tray")]
pub(crate) struct TrayRegistration {
    pub options: TrayOptions,
    pub handler: TrayCommandHandler,
}

#[cfg(feature = "notifications")]
pub(crate) struct NotificationRegistration {
    pub(crate) identity: String,
}

#[derive(Clone)]
pub struct WindowManager {
    inner: Arc<WindowManagerInner>,
}

struct WindowManagerInner {
    command: RwLock<Option<WindowCommandHandler>>,
}

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
        input: crate::core::InputEvent,
    },
    Exit,
}

impl WindowManager {
    fn new() -> Self {
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

    pub fn send_input(&self, id: impl Into<WindowId>, input: crate::core::InputEvent) -> bool {
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
}

#[derive(Clone)]
pub struct ApplicationContext {
    inner: Arc<ApplicationContextInner>,
}

struct ApplicationContextInner {
    resources: Resources,
    executor: RwLock<Option<UiTaskSpawner>>,
    commands: CommandRegistry,
    events: EventBus,
    #[cfg(feature = "store")]
    stores: Arc<StoreRuntime>,
    #[cfg(feature = "router")]
    routers: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    windows: WindowManager,
}

impl ApplicationContext {
    pub fn empty() -> Self {
        Self::new(
            Resources::new(),
            None,
            CommandRegistry::default(),
            EventBus::default(),
        )
    }

    fn new(
        resources: Resources,
        executor: Option<UiTaskSpawner>,
        commands: CommandRegistry,
        events: EventBus,
    ) -> Self {
        #[cfg(feature = "store")]
        let stores = Arc::new(StoreRuntime::new(resources.clone()));
        Self {
            inner: Arc::new(ApplicationContextInner {
                resources,
                executor: RwLock::new(executor),
                commands,
                events,
                #[cfg(feature = "store")]
                stores,
                #[cfg(feature = "router")]
                routers: Mutex::new(HashMap::new()),
                windows: WindowManager::new(),
            }),
        }
    }

    pub fn resources(&self) -> &Resources {
        &self.inner.resources
    }

    pub fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.inner.resources.require::<T>()
    }

    pub fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.inner.resources.get::<T>()
    }

    pub fn command<C>(&self) -> CommandHandle<C>
    where
        C: Command,
    {
        CommandHandle::new(self.clone())
    }

    pub async fn invoke<C>(&self, args: C::Args) -> Result<C::Output, C::Error>
    where
        C: Command,
    {
        self.command::<C>().invoke(args).await
    }

    pub fn emit<E>(&self, event: E) -> usize
    where
        E: Event,
    {
        self.inner.events.emit(event)
    }

    pub fn subscribe<E>(&self, listener: impl Fn(E) + Send + Sync + 'static) -> EventSubscription
    where
        E: Event,
    {
        self.inner.events.subscribe(listener)
    }

    pub(crate) fn command_registry(&self) -> &CommandRegistry {
        &self.inner.commands
    }

    #[cfg(feature = "store")]
    pub fn stores(&self) -> &Arc<StoreRuntime> {
        &self.inner.stores
    }

    #[cfg(feature = "store")]
    pub fn read_store<T, R>(&self, read: impl FnOnce(&T) -> R) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.inner.stores.read(read)
    }

    #[cfg(feature = "store")]
    pub fn update_store<T, R>(
        &self,
        reason: impl Into<std::borrow::Cow<'static, str>>,
        update: impl FnOnce(&mut T) -> R,
    ) -> R
    where
        T: crate::store::StoreUnit,
    {
        self.inner.stores.update(reason, update)
    }

    #[cfg(feature = "router")]
    pub fn router<R>(&self) -> Router<R>
    where
        R: Default + Clone + PartialEq + Send + Sync + 'static,
    {
        let mut routers = self.inner.routers.lock().expect("router registry poisoned");
        routers
            .entry(TypeId::of::<R>())
            .or_insert_with(|| Arc::new(Router::new(R::default())))
            .clone()
            .downcast::<Router<R>>()
            .unwrap_or_else(|_| panic!("router registry type mismatch"))
            .as_ref()
            .clone()
    }

    pub fn set_executor(&self, executor: UiTaskSpawner) {
        *self.inner.executor.write().expect("UI executor poisoned") = Some(executor);
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) -> bool {
        let Some(executor) = self
            .inner
            .executor
            .read()
            .expect("UI executor poisoned")
            .clone()
        else {
            return false;
        };
        executor.spawn(Box::pin(task));
        true
    }

    pub fn windows(&self) -> WindowManager {
        self.inner.windows.clone()
    }

    pub(crate) fn report_render_error(&self, error: RenderError) {
        if let Some(registration) = self.try_resource::<RenderErrorRegistration>() {
            (registration.handler)(&error);
        } else {
            eprintln!("{error}");
        }
    }

    #[cfg(feature = "notifications")]
    pub fn notifications(&self) -> Option<Arc<NotificationHandle>> {
        self.try_resource::<NotificationHandle>()
    }

    #[cfg(feature = "clipboard")]
    pub fn clipboard(&self) -> crate::platform::ClipboardHandle {
        (*self.resource::<crate::platform::ClipboardHandle>()).clone()
    }

    #[cfg(feature = "open-url")]
    pub fn open_url(&self, url: &str) -> Result<(), crate::desktop::OpenUrlError> {
        self.resource::<crate::desktop::OpenUrlHandle>().open(url)
    }

    #[cfg(feature = "dialogs")]
    pub fn file_dialogs(&self) -> Arc<crate::dialogs::FileDialogHandle> {
        self.resource::<crate::dialogs::FileDialogHandle>()
    }

    pub(crate) fn task_spawner(&self) -> Option<UiTaskSpawner> {
        self.inner
            .executor
            .read()
            .expect("UI executor poisoned")
            .clone()
    }
}

impl PartialEq for ApplicationContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

struct ApplicationRoot {
    context: ApplicationContext,
    view: AppView,
}

impl RootComponent for ApplicationRoot {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> Element {
        let viewport = cx.viewport();
        let view = self.view;
        let content = component(viewport, move |cx, _| view(cx)).key("lgui.application.root");
        #[cfg(feature = "store")]
        let content = context_provider(
            StoreContext::new(Arc::clone(self.context.stores())),
            content,
        );
        context_provider(self.context, content)
    }
}

pub(crate) fn application_root_view(context: ApplicationContext, view: AppView) -> AppView {
    Arc::new(move |cx| {
        ApplicationRoot {
            context: context.clone(),
            view: Arc::clone(&view),
        }
        .render_root(cx)
    })
}

/// Cloneable, platform-neutral access to the running application event loop.
#[derive(Clone)]
pub struct ApplicationHandle {
    post: Arc<dyn Fn(ApplicationTask) + Send + Sync>,
    request_frame: Arc<dyn Fn() + Send + Sync>,
}

impl ApplicationHandle {
    pub fn new(
        post: impl Fn(ApplicationTask) + Send + Sync + 'static,
        request_frame: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            post: Arc::new(post),
            request_frame: Arc::new(request_frame),
        }
    }

    pub fn post(&self, task: impl FnOnce() + Send + 'static) {
        (self.post)(Box::new(task));
    }

    pub fn request_frame(&self) {
        (self.request_frame)();
    }
}

impl RootComponent for AppView {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> Element {
        self(cx)
    }
}

pub type WindowDragExclusion = fn(f32, f32) -> UiRect;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(String);

impl WindowId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        assert!(!value.trim().is_empty(), "window id must not be empty");
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for WindowId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for WindowId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A renderer failure reported at the application boundary.
///
/// Rendering backends remain independent from the application's logging stack. Applications can
/// install a handler with [`Application::on_render_error`] and decide whether a failure belongs in
/// a local log, diagnostics UI, telemetry, or another reporting destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
    window: WindowId,
    renderer: &'static str,
    stage: RenderErrorStage,
    operation: &'static str,
    code: i32,
    message: String,
}

impl RenderError {
    pub(crate) fn new(
        window: WindowId,
        renderer: &'static str,
        stage: RenderErrorStage,
        operation: &'static str,
        code: i32,
        message: impl Into<String>,
    ) -> Self {
        Self {
            window,
            renderer,
            stage,
            operation,
            code,
            message: message.into(),
        }
    }

    pub fn window(&self) -> &WindowId {
        &self.window
    }

    pub const fn renderer(&self) -> &'static str {
        self.renderer
    }

    pub const fn stage(&self) -> RenderErrorStage {
        self.stage
    }

    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    pub const fn code(&self) -> i32 {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "renderer '{}' failed to {} window '{}' during {} (0x{:08X}): {}",
            self.renderer,
            self.operation,
            self.window.as_str(),
            self.stage.as_str(),
            self.code as u32,
            self.message
        )
    }
}

impl std::error::Error for RenderError {}

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

pub type WindowCloseHandler = fn(&mut crate::core::UiEventContext);

#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererKind {
    #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
    Gdi,
    #[cfg(all(feature = "renderer-d2d", target_os = "windows"))]
    D2d,
    #[cfg(feature = "renderer-skia")]
    Skia(GraphicsPreference),
}

#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
impl Default for RendererKind {
    fn default() -> Self {
        #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
        {
            return Self::Gdi;
        }
        #[cfg(all(
            not(all(feature = "renderer-gdi", target_os = "windows")),
            feature = "renderer-d2d",
            target_os = "windows"
        ))]
        {
            return Self::D2d;
        }
        #[cfg(all(
            not(all(feature = "renderer-gdi", target_os = "windows")),
            not(all(feature = "renderer-d2d", target_os = "windows")),
            feature = "renderer-skia"
        ))]
        {
            Self::Skia(GraphicsPreference::Auto)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsPreference {
    #[default]
    Auto,
    OpenGl,
    Vulkan,
    Metal,
    Software,
}

impl GraphicsPreference {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::OpenGl => "opengl",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::Software => "software",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererProbeError(String);

impl std::fmt::Display for RendererProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RendererProbeError {}

#[cfg(any(
    all(feature = "renderer-gdi", target_os = "windows"),
    all(feature = "renderer-d2d", target_os = "windows"),
    feature = "renderer-skia"
))]
impl RendererKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
            Self::Gdi => "gdi",
            #[cfg(all(feature = "renderer-d2d", target_os = "windows"))]
            Self::D2d => "d2d",
            #[cfg(feature = "renderer-skia")]
            Self::Skia(_) => "skia",
        }
    }

    pub fn probe(self) -> Result<(), RendererProbeError> {
        match self {
            #[cfg(all(feature = "renderer-gdi", target_os = "windows"))]
            Self::Gdi => Ok(()),
            #[cfg(all(feature = "renderer-d2d", target_os = "windows"))]
            Self::D2d => crate::platform::win32::probe_d2d_support()
                .map_err(|error| RendererProbeError(error.to_string())),
            #[cfg(feature = "renderer-skia")]
            Self::Skia(preference) => {
                crate::platform::skia::probe_skia_support(preference).map_err(RendererProbeError)
            }
        }
    }
}

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

pub trait ApplicationBackend: Sized {
    type Error;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error>;
}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
pub enum DesktopApplication {
    Win32(crate::platform::win32::Win32Application),
    Winit(crate::platform::WinitApplication),
}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
#[derive(Debug)]
pub struct DesktopApplicationError(String);

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl std::fmt::Display for DesktopApplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl std::error::Error for DesktopApplicationError {}

#[cfg(all(
    target_os = "windows",
    feature = "renderer-gdi",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl ApplicationBackend for DesktopApplication {
    type Error = DesktopApplicationError;

    fn run(
        self,
        options: WindowOptions,
        view: AppView,
        context: ApplicationContext,
    ) -> Result<(), Self::Error> {
        match self {
            Self::Win32(backend) => backend
                .run(options, view, context)
                .map_err(|error| DesktopApplicationError(error.to_string())),
            Self::Winit(backend) => backend
                .run(options, view, context)
                .map_err(|error| DesktopApplicationError(error.to_string())),
        }
    }
}

pub struct Application<B> {
    backend: B,
    window: WindowOptions,
    resources: Resources,
    executor: Option<UiTaskSpawner>,
    commands: CommandRegistry,
    events: EventBus,
}

impl<B> Application<B> {
    pub fn with_backend(backend: B) -> Self {
        Self {
            backend,
            window: WindowOptions::default(),
            resources: Resources::new(),
            executor: None,
            commands: CommandRegistry::default(),
            events: EventBus::default(),
        }
    }

    pub fn window_options(mut self, options: WindowOptions) -> Self {
        self.window = options;
        self
    }

    pub fn provide<T>(self, value: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.resources.provide(value);
        self
    }

    pub fn executor(mut self, executor: impl UiExecutor) -> Self {
        self.executor = Some(Arc::new(executor));
        self
    }

    pub fn command<C>(self, handler: impl CommandHandler<C>) -> Self
    where
        C: Command,
    {
        assert!(
            self.commands.register::<C>(handler),
            "command `{}` is already registered",
            C::NAME
        );
        self
    }

    pub fn on_render_error(self, handler: impl Fn(&RenderError) + Send + Sync + 'static) -> Self {
        self.resources.provide(RenderErrorRegistration {
            handler: Arc::new(handler),
        });
        self
    }

    #[cfg(feature = "diagnostics")]
    pub fn diagnostics_sink(
        self,
        sink: impl crate::diagnostics::DiagnosticsSink + 'static,
    ) -> Self {
        self.resources
            .provide(crate::diagnostics::DiagnosticsRegistration::new(sink));
        self
    }

    pub fn font_families(self, families: &'static [&'static str]) -> Self {
        self.resources.provide(crate::text::FontFamilies(families));
        self
    }

    pub fn font_assets(self, assets: Vec<crate::text::FontAsset>) -> Self {
        self.resources
            .provide(crate::text::FontAssets(std::sync::Arc::new(assets)));
        self
    }

    #[cfg(feature = "svg")]
    pub fn svg_icons(self, registry: crate::icons::SvgIconRegistry) -> Self {
        self.resources
            .provide(crate::icons::IconRegistration(std::sync::Arc::new(
                registry,
            )));
        self
    }

    #[cfg(feature = "tray")]
    pub fn tray(
        self,
        options: TrayOptions,
        handler: impl Fn(&ApplicationContext, &str) + Send + Sync + 'static,
    ) -> Self {
        self.resources.provide(TrayRegistration {
            options,
            handler: Arc::new(handler),
        });
        self
    }

    #[cfg(feature = "notifications")]
    pub fn notifications(self, identity: impl Into<String>) -> Self {
        self.resources.provide(NotificationRegistration {
            identity: identity.into(),
        });
        self
    }
}

#[cfg(all(
    feature = "renderer-gdi",
    target_os = "windows",
    not(all(feature = "backend-winit", feature = "renderer-skia"))
))]
impl Application<crate::platform::win32::Win32Application> {
    pub fn new() -> Self {
        Self::with_backend(crate::platform::win32::Win32Application::default())
            .provide(RendererKind::Gdi)
    }

    pub fn renderer(mut self, renderer: RendererKind) -> Self {
        self.resources.provide(renderer);
        self.backend = match renderer {
            RendererKind::Gdi => crate::platform::win32::Win32Application::with_renderer(
                crate::platform::win32::GdiRendererFactory,
            ),
            #[cfg(feature = "renderer-d2d")]
            RendererKind::D2d => crate::platform::win32::Win32Application::with_renderer(
                crate::platform::win32::D2dRendererFactory,
            ),
            #[cfg(feature = "renderer-skia")]
            RendererKind::Skia(_) => {
                panic!("the Skia desktop renderer requires the backend-winit feature")
            }
        };
        self
    }
}

#[cfg(all(
    feature = "renderer-gdi",
    target_os = "windows",
    feature = "backend-winit",
    feature = "renderer-skia"
))]
impl Application<DesktopApplication> {
    pub fn new() -> Self {
        Self::with_backend(DesktopApplication::Win32(
            crate::platform::win32::Win32Application::default(),
        ))
        .provide(RendererKind::Gdi)
    }

    pub fn renderer(mut self, renderer: RendererKind) -> Self {
        self.resources.provide(renderer);
        self.backend = match renderer {
            RendererKind::Gdi => {
                DesktopApplication::Win32(crate::platform::win32::Win32Application::with_renderer(
                    crate::platform::win32::GdiRendererFactory,
                ))
            }
            #[cfg(feature = "renderer-d2d")]
            RendererKind::D2d => {
                DesktopApplication::Win32(crate::platform::win32::Win32Application::with_renderer(
                    crate::platform::win32::D2dRendererFactory,
                ))
            }
            RendererKind::Skia(preference) => {
                DesktopApplication::Winit(crate::platform::WinitApplication::new(preference))
            }
        };
        self
    }
}

#[cfg(all(feature = "renderer-skia", feature = "backend-winit"))]
impl Application<crate::platform::WinitApplication> {
    pub fn new_skia(preference: GraphicsPreference) -> Self {
        Self::with_backend(crate::platform::WinitApplication::new(preference))
            .provide(RendererKind::Skia(preference))
    }
}

impl<B> Application<B>
where
    B: ApplicationBackend,
{
    pub fn run(
        self,
        view: impl for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
            + Send
            + Sync
            + 'static,
    ) -> Result<(), B::Error> {
        #[cfg(feature = "clipboard")]
        if self
            .resources
            .get::<crate::platform::ClipboardHandle>()
            .is_none()
        {
            self.resources
                .provide::<crate::platform::ClipboardHandle>(crate::clipboard::system_clipboard());
        }
        #[cfg(feature = "open-url")]
        if self
            .resources
            .get::<crate::desktop::OpenUrlHandle>()
            .is_none()
        {
            self.resources.provide(crate::desktop::system_url_opener());
        }
        #[cfg(feature = "dialogs")]
        if self
            .resources
            .get::<crate::dialogs::FileDialogHandle>()
            .is_none()
        {
            self.resources
                .provide(crate::dialogs::system_file_dialogs());
        }
        let context =
            ApplicationContext::new(self.resources, self.executor, self.commands, self.events);
        let backend_context = context.clone();
        let view: AppView = Arc::new(view);
        let root = application_root_view(context, view);
        self.backend.run(self.window, root, backend_context)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        task::{Context, Poll, Waker},
    };

    use super::*;

    struct RecordingBackend(Arc<Mutex<Option<WindowOptions>>>);

    struct ReportingBackend;

    struct BuilderCommand;

    impl Command for BuilderCommand {
        type Args = usize;
        type Output = usize;
        type Error = ();

        const NAME: &'static str = "test.builder";
    }

    struct InvokingBackend(Arc<Mutex<Option<usize>>>);

    impl ApplicationBackend for RecordingBackend {
        type Error = ();

        fn run(
            self,
            options: WindowOptions,
            _view: AppView,
            _context: ApplicationContext,
        ) -> Result<(), Self::Error> {
            *self.0.lock().expect("window options lock poisoned") = Some(options);
            Ok(())
        }
    }

    impl ApplicationBackend for ReportingBackend {
        type Error = ();

        fn run(
            self,
            _options: WindowOptions,
            _view: AppView,
            context: ApplicationContext,
        ) -> Result<(), Self::Error> {
            context.report_render_error(RenderError::new(
                WindowId::new("reporting-window"),
                "test",
                RenderErrorStage::Present,
                "present_test_frame",
                0x80004005_u32 as i32,
                "synthetic present failure",
            ));
            Ok(())
        }
    }

    impl ApplicationBackend for InvokingBackend {
        type Error = ();

        fn run(
            self,
            _options: WindowOptions,
            _view: AppView,
            context: ApplicationContext,
        ) -> Result<(), Self::Error> {
            let mut future = Box::pin(context.invoke::<BuilderCommand>(41));
            let waker = Waker::noop();
            let mut task_context = Context::from_waker(waker);
            let Poll::Ready(result) = future.as_mut().poll(&mut task_context) else {
                panic!("test command unexpectedly pending");
            };
            *self.0.lock().expect("command result lock poisoned") = Some(result?);
            Ok(())
        }
    }

    #[test]
    fn application_builder_registers_commands_on_the_runtime_context() {
        let result = Arc::new(Mutex::new(None));

        Application::with_backend(InvokingBackend(Arc::clone(&result)))
            .command::<BuilderCommand>(|_, value| async move { Ok(value + 1) })
            .run(|cx| crate::core::group(cx.viewport()))
            .expect("invoking backend should run");

        assert_eq!(
            *result.lock().expect("command result lock poisoned"),
            Some(42)
        );
    }

    #[test]
    fn render_errors_are_delivered_with_structured_context() {
        let errors = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&errors);

        Application::with_backend(ReportingBackend)
            .on_render_error(move |error| {
                captured
                    .lock()
                    .expect("render error capture poisoned")
                    .push(error.clone());
            })
            .run(|cx| crate::core::group(cx.viewport()))
            .expect("reporting backend should run");

        let errors = errors.lock().expect("render error capture poisoned");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].window().as_str(), "reporting-window");
        assert_eq!(errors[0].renderer(), "test");
        assert_eq!(errors[0].stage(), RenderErrorStage::Present);
        assert_eq!(errors[0].operation(), "present_test_frame");
        assert_eq!(errors[0].code(), 0x80004005_u32 as i32);
        assert_eq!(errors[0].message(), "synthetic present failure");
    }

    #[test]
    fn application_passes_window_options_to_the_backend() {
        let recorded = Arc::new(Mutex::new(None));
        let options = WindowOptions::new("counter")
            .size(Size::new(640.0, 480.0))
            .minimum_size(Size::new(320.0, 240.0))
            .maximum_size(Size::new(1280.0, 960.0))
            .resizable(false);

        Application::with_backend(RecordingBackend(Arc::clone(&recorded)))
            .window_options(options.clone())
            .run(|_| crate::core::group(crate::core::UiRect::new(0.0, 0.0, 1.0, 1.0)))
            .expect("mock backend should run");

        let recorded = recorded
            .lock()
            .expect("window options lock poisoned")
            .clone()
            .expect("backend should receive window options");
        assert_eq!(recorded.id, options.id);
        assert_eq!(recorded.size, options.size);
        assert_eq!(recorded.minimum_size, options.minimum_size);
        assert_eq!(recorded.maximum_size, options.maximum_size);
        assert_eq!(recorded.resizable, options.resizable);
    }

    #[test]
    fn native_titlebar_is_enabled_by_default_and_can_be_disabled() {
        assert!(WindowOptions::default().native_titlebar);
        assert!(WindowOptions::new("default").native_titlebar);
        assert!(
            !WindowOptions::new("custom")
                .native_titlebar(false)
                .native_titlebar
        );
    }

    #[test]
    fn windows_are_visible_by_default_and_can_start_hidden() {
        assert!(WindowOptions::default().visible);
        assert!(!WindowOptions::new("background").visible(false).visible);
    }

    #[test]
    fn window_size_constraints_are_optional_and_configurable() {
        let defaults = WindowOptions::new("default");
        assert_eq!(defaults.minimum_size, None);
        assert_eq!(defaults.maximum_size, None);

        let constrained = defaults
            .minimum_size(Size::new(640.0, 360.0))
            .maximum_size(Size::new(1920.0, 1080.0));
        assert_eq!(constrained.minimum_size, Some(Size::new(640.0, 360.0)));
        assert_eq!(constrained.maximum_size, Some(Size::new(1920.0, 1080.0)));
    }

    #[test]
    fn application_handle_posts_tasks_and_requests_frames() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let posted = Arc::new(AtomicUsize::new(0));
        let frames = Arc::new(AtomicUsize::new(0));
        let handle = ApplicationHandle::new(
            {
                let posted = Arc::clone(&posted);
                move |task| {
                    task();
                    posted.fetch_add(1, Ordering::SeqCst);
                }
            },
            {
                let frames = Arc::clone(&frames);
                move || {
                    frames.fetch_add(1, Ordering::SeqCst);
                }
            },
        );

        handle.post(|| {});
        handle.request_frame();

        assert_eq!(posted.load(Ordering::SeqCst), 1);
        assert_eq!(frames.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn auxiliary_window_options_are_declared_without_platform_handles() {
        let options = WindowOptions::new("friends")
            .owner("main")
            .title("Friends")
            .size(Size::new(292.0, 640.0))
            .minimum_size(Size::new(292.0, 360.0))
            .position(WindowPosition::AdjacentToOwner { gap: 1 })
            .transparent(true)
            .corner_radius(8)
            .background_memory_optimization(true);

        assert_eq!(options.id.as_str(), "friends");
        assert_eq!(options.owner.as_ref().map(WindowId::as_str), Some("main"));
        assert_eq!(options.position, WindowPosition::AdjacentToOwner { gap: 1 });
        assert!(options.transparent);
        assert!(options.background_memory_optimization);
    }

    #[test]
    fn window_manager_marshals_the_complete_command_model() {
        let manager = WindowManager::new();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&commands);
        manager.install(move |command| {
            let label = match command {
                WindowCommand::Show { options, .. } => format!("show:{}", options.id.as_str()),
                WindowCommand::Hide(id) => format!("hide:{}", id.as_str()),
                WindowCommand::Toggle { options, .. } => {
                    format!("toggle:{}", options.id.as_str())
                }
                WindowCommand::Close(id) => format!("close:{}", id.as_str()),
                WindowCommand::RequestClose(id) => format!("request-close:{}", id.as_str()),
                WindowCommand::Minimize(id) => format!("minimize:{}", id.as_str()),
                WindowCommand::SetScalePreference(ScalePreference::Auto) => "scale:auto".into(),
                WindowCommand::SetScalePreference(ScalePreference::Multiplier(value)) => {
                    format!("scale:{value}")
                }
                WindowCommand::SetMode { id, mode } => {
                    format!("mode:{}:{mode:?}", id.as_str())
                }
                WindowCommand::Input { id, .. } => format!("input:{}", id.as_str()),
                WindowCommand::Exit => "exit".into(),
            };
            recorded.lock().expect("command log poisoned").push(label);
        });

        manager.show(WindowOptions::new("friends"), |_| {
            crate::core::content_text("friends")
        });
        manager.hide("friends");
        manager.toggle(WindowOptions::new("friends"), |_| {
            crate::core::content_text("friends")
        });
        manager.set_scale_preference(ScalePreference::Multiplier(0.9));
        manager.set_mode("friends", WindowMode::Fullscreen);
        manager.request_close("friends");
        manager.close("friends");
        manager.exit();

        assert_eq!(
            *commands.lock().expect("command log poisoned"),
            [
                "show:friends",
                "hide:friends",
                "toggle:friends",
                "scale:0.9",
                "mode:friends:Fullscreen",
                "request-close:friends",
                "close:friends",
                "exit",
            ]
        );
    }

    struct RenderingBackend(Arc<AtomicUsize>);

    impl ApplicationBackend for RenderingBackend {
        type Error = ();

        fn run(
            self,
            _options: WindowOptions,
            view: AppView,
            _context: ApplicationContext,
        ) -> Result<(), Self::Error> {
            let mut session = crate::session::UiSession::new();
            let _ = session.render_view(
                &view,
                UiRect::new(0.0, 0.0, 320.0, 200.0),
                crate::core::UiScale::ONE,
            );
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn application_run_mounts_and_renders_the_root_component() {
        let backend_frames = Arc::new(AtomicUsize::new(0));
        let root_executions = Arc::new(AtomicUsize::new(0));
        let executions = Arc::clone(&root_executions);

        Application::with_backend(RenderingBackend(Arc::clone(&backend_frames)))
            .run(move |cx| {
                let _application = cx.application();
                executions.fetch_add(1, Ordering::SeqCst);
                crate::core::content_text("root")
            })
            .expect("recording backend should render");

        assert_eq!(backend_frames.load(Ordering::SeqCst), 1);
        assert_eq!(root_executions.load(Ordering::SeqCst), 1);
    }

    #[cfg(feature = "store")]
    mod store_lifecycle {
        use super::*;
        use crate::{resources::Resources, store::StoreUnit};

        struct CreateCounter(Arc<AtomicUsize>);

        struct LazyStore;

        impl StoreUnit for LazyStore {
            const KEY: &'static str = "test.lazy";

            fn create(resources: &Resources) -> Self {
                resources
                    .require::<CreateCounter>()
                    .0
                    .fetch_add(1, Ordering::SeqCst);
                Self
            }
        }

        struct ValueStore(i32);

        impl StoreUnit for ValueStore {
            const KEY: &'static str = "test.value";

            fn create(_resources: &Resources) -> Self {
                Self(0)
            }
        }

        struct DroppedStore(Arc<AtomicUsize>);

        impl Drop for DroppedStore {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        impl StoreUnit for DroppedStore {
            const KEY: &'static str = "test.drop";

            fn create(resources: &Resources) -> Self {
                Self(resources.require::<CreateCounter>().0.clone())
            }
        }

        fn context_with_counter(counter: Arc<AtomicUsize>) -> ApplicationContext {
            let resources = Resources::new();
            resources.provide(CreateCounter(counter));
            ApplicationContext::new(
                resources,
                None,
                CommandRegistry::default(),
                EventBus::default(),
            )
        }

        #[test]
        fn stores_are_created_once_on_first_use() {
            let creations = Arc::new(AtomicUsize::new(0));
            let application = context_with_counter(Arc::clone(&creations));

            assert_eq!(creations.load(Ordering::SeqCst), 0);
            application.read_store::<LazyStore, _>(|_| ());
            application.read_store::<LazyStore, _>(|_| ());
            assert_eq!(creations.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn store_instances_are_isolated_per_application() {
            let first = ApplicationContext::empty();
            let second = ApplicationContext::empty();

            first.update_store::<ValueStore, _>("first", |store| store.0 = 7);

            assert_eq!(first.read_store::<ValueStore, _>(|store| store.0), 7);
            assert_eq!(second.read_store::<ValueStore, _>(|store| store.0), 0);
        }

        #[test]
        fn store_updates_wake_the_owning_application() {
            let application = ApplicationContext::empty();
            let wakes = Arc::new(AtomicUsize::new(0));
            application.stores().set_wake({
                let wakes = Arc::clone(&wakes);
                Arc::new(move || {
                    wakes.fetch_add(1, Ordering::SeqCst);
                })
            });

            application.update_store::<ValueStore, _>("wake", |store| store.0 += 1);

            assert_eq!(wakes.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn application_drop_releases_its_store_instances() {
            let drops = Arc::new(AtomicUsize::new(0));
            {
                let application = context_with_counter(Arc::clone(&drops));
                application.read_store::<DroppedStore, _>(|_| ());
                assert_eq!(drops.load(Ordering::SeqCst), 0);
            }
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[cfg(all(
        feature = "renderer-gdi",
        feature = "renderer-d2d",
        target_os = "windows"
    ))]
    #[test]
    fn gdi_and_d2d_use_the_same_application_builder_type() {
        #[cfg(not(all(feature = "backend-winit", feature = "renderer-skia")))]
        fn assert_type(_: Application<crate::platform::win32::Win32Application>) {}
        #[cfg(all(feature = "backend-winit", feature = "renderer-skia"))]
        fn assert_type(_: Application<DesktopApplication>) {}

        assert_type(Application::new().renderer(RendererKind::Gdi));
        assert_type(Application::new().renderer(RendererKind::D2d));
    }
}
