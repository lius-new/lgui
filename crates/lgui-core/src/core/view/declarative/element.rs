use super::*;

// Declarative core shell only: this layer owns tree identity and composition,
// but must not depend on concrete UI components. Component-specific adapters
// belong beside the component they wrap during the migration.
pub trait DeclarativeView {
    fn compile(
        self,
        scope: &UiScope,
        context: &UiRenderContext,
        component_id: ComponentId,
        force_components: bool,
    ) -> UiElement;
}

pub trait IntoElementContent {
    fn append_to(self, children: &mut Vec<Element>);
}

pub struct Fragment {
    pub(super) children: Vec<Element>,
}

pub struct Element {
    pub(super) key: ElementKey,
    render: Box<dyn FnOnce(ElementRenderCx<'_, '_, '_>) -> UiElement>,
    interaction: Option<InteractionRole>,
    cursor: Option<CursorIcon>,
    semantics: Option<Semantics>,
    pub(super) click_capture_handler: Option<UiEventHandler>,
    pub(super) click_handler: Option<UiEventHandler>,
    pub(super) input_event_handlers: Vec<UiInputEventBinding>,
    pub(super) event_policy: Option<EventPolicy>,
    phase: Option<RenderPhase>,
    animations: Vec<AnimationBinding>,
    animation_targets: Vec<(AnimProperty, bool)>,
    paint_bounds: Option<UiRect>,
    shadow: Option<crate::core::ShadowStyle>,
    focus_scope: bool,
    defer_children: bool,
    children: Vec<Element>,
}

#[derive(Clone, Copy)]
pub struct ElementKey {
    file: &'static str,
    line: u32,
    column: u32,
    explicit: Option<u64>,
}

pub struct ElementRenderCx<'a, 'ctx, 'scope> {
    pub id: UiId,
    pub scope: &'scope UiScope,
    pub context: &'ctx UiRenderContext<'a>,
    pub children: Vec<UiElement>,
    pub(super) component_id: ComponentId,
    pub(super) force_components: bool,
    deferred_children: Option<Vec<Element>>,
    id_counter: Cell<u32>,
}

impl ElementRenderCx<'_, '_, '_> {
    /// Returns a unique auto-generated child ID within the current scope.
    /// Use this instead of manually calling `cx.scope.id(format!(...))`.
    pub fn auto_id(&self) -> UiId {
        let n = self.id_counter.get();
        self.id_counter.set(n + 1);
        self.scope.id(format!("{}._{}", self.id.as_str(), n))
    }

    pub fn use_context<T>(&self) -> T
    where
        T: Clone + 'static,
    {
        self.try_use_context::<T>().unwrap_or_else(|| {
            panic!(
                "missing context value `{}` for element `{}`",
                type_name::<T>(),
                self.id.as_str()
            )
        })
    }

    pub fn try_use_context<T>(&self) -> Option<T>
    where
        T: Clone + 'static,
    {
        self.context.contexts().read(self.component_id)
    }

    #[doc(hidden)]
    pub fn component_owner(&self) -> (ComponentId, bool) {
        (self.component_id, self.force_components)
    }
}

impl Element {
    #[track_caller]
    pub fn new(render: impl FnOnce(ElementRenderCx<'_, '_, '_>) -> UiElement + 'static) -> Self {
        Self::with_key(ElementKey::caller(), render)
    }

    pub fn with_key(
        key: ElementKey,
        render: impl FnOnce(ElementRenderCx<'_, '_, '_>) -> UiElement + 'static,
    ) -> Self {
        Self {
            key,
            render: Box::new(render),
            interaction: None,
            cursor: None,
            semantics: None,
            click_capture_handler: None,
            click_handler: None,
            input_event_handlers: Vec::new(),
            event_policy: None,
            phase: None,
            animations: Vec::new(),
            animation_targets: Vec::new(),
            paint_bounds: None,
            shadow: None,
            focus_scope: false,
            defer_children: false,
            children: Vec::new(),
        }
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn content(mut self, content: impl IntoElementContent) -> Self {
        content.append_to(&mut self.children);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Element>>) -> Self {
        self.children.extend(children.into_iter().map(Into::into));
        self
    }

    pub fn key(mut self, key: impl AsRef<str>) -> Self {
        self.key.explicit = Some(stable_hash(key.as_ref()));
        self
    }

    #[doc(hidden)]
    pub fn source_key(mut self, key: ElementKey) -> Self {
        self.key = key;
        self
    }

    pub fn interaction(mut self, interaction: InteractionRole) -> Self {
        self.interaction = Some(interaction);
        self
    }

    pub fn cursor(mut self, cursor: CursorIcon) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn semantics(mut self, semantics: Semantics) -> Self {
        self.semantics = Some(semantics);
        self
    }

    /// Marks this element as a native window drag region.
    ///
    /// Interactive descendants are excluded automatically by hit testing.
    pub fn window_drag_region(mut self) -> Self {
        self.interaction = Some(InteractionRole::WindowDragRegion);
        self.event_policy = Some(EventPolicy::NONE);
        self
    }

    pub fn phase(mut self, phase: RenderPhase) -> Self {
        self.phase = Some(phase);
        self
    }

    pub fn animation(mut self, binding: AnimationBinding) -> Self {
        self.animations.push(binding);
        self
    }

    pub fn animation_target(mut self, property: AnimProperty, active: bool) -> Self {
        self.animation_targets.push((property, active));
        self
    }

    pub fn paint_bounds(mut self, rect: UiRect) -> Self {
        self.paint_bounds = Some(rect);
        self
    }

    /// Applies one shadow to the composited element and subtree. Ancestor clips still apply.
    pub fn shadow(mut self, style: crate::core::ShadowStyle) -> Self {
        self.shadow = Some(style);
        self
    }

    pub fn focus_scope(mut self) -> Self {
        self.focus_scope = true;
        self
    }

    pub fn defer_children_compile(mut self) -> Self {
        self.defer_children = true;
        self
    }

    fn compile_internal(
        self,
        scope: &UiScope,
        context: &UiRenderContext,
        component_id: ComponentId,
        force_components: bool,
    ) -> UiElement {
        let _current_context = context.contexts().enter_current(component_id);
        let (children, deferred_children) = if self.defer_children {
            (Vec::new(), Some(self.children))
        } else {
            (
                compile_children(
                    scope,
                    context,
                    component_id,
                    force_components,
                    self.children,
                ),
                None,
            )
        };
        let mut element = (self.render)(ElementRenderCx {
            id: scope.node_id(),
            scope,
            context,
            children,
            component_id,
            force_components,
            deferred_children,
            id_counter: Cell::new(0),
        });
        if let Some(interaction) = self.interaction {
            element = element.interaction(interaction);
        }
        if let Some(cursor) = self.cursor {
            element = element.cursor(cursor);
        }
        if let Some(semantics) = self.semantics {
            element = element.semantics(semantics);
        }
        if let Some(handler) = self.click_capture_handler {
            element = element.on_click_capture_handler(handler);
        }
        if let Some(handler) = self.click_handler {
            element = element.on_click_handler(handler);
        }
        for binding in self.input_event_handlers {
            element = element.on_event_handler(binding.kind, binding.capture, binding.handler);
        }
        if let Some(policy) = self.event_policy {
            element = element.event_policy(policy);
        }
        if let Some(phase) = self.phase {
            element = element.render_phase(phase);
        }
        for animation in self.animations {
            element = element.animation(animation);
        }
        for (property, active) in self.animation_targets {
            element = element.animation_target(property, active);
        }
        if let Some(bounds) = self.paint_bounds {
            element = element.paint_bounds(bounds);
        }
        if self.focus_scope {
            element = element.focus_scope();
        }
        if let Some(shadow) = self.shadow {
            element = element.shadow(shadow);
        }
        element
    }
}

impl ElementRenderCx<'_, '_, '_> {
    pub fn animation_value(&self, property: AnimProperty) -> f32 {
        self.context.animation_value(&self.id, property)
    }

    pub fn compile(&self, element: Element) -> UiElement {
        element.compile_internal(
            self.scope,
            self.context,
            self.component_id,
            self.force_components,
        )
    }

    pub fn compile_deferred_children(&mut self) -> Vec<UiElement> {
        self.deferred_children
            .take()
            .map(|children| {
                compile_children(
                    self.scope,
                    self.context,
                    self.component_id,
                    self.force_components,
                    children,
                )
            })
            .unwrap_or_default()
    }
}

impl DeclarativeView for Element {
    fn compile(
        self,
        scope: &UiScope,
        context: &UiRenderContext,
        component_id: ComponentId,
        force_components: bool,
    ) -> UiElement {
        self.compile_internal(scope, context, component_id, force_components)
    }
}

impl ElementKey {
    #[track_caller]
    pub fn caller() -> Self {
        let location = Location::caller();
        Self {
            file: location.file(),
            line: location.line(),
            column: location.column(),
            explicit: None,
        }
    }

    pub(super) fn segment(self, index: usize) -> String {
        match self.explicit {
            Some(key) => format!("e.{:016x}.k.{key:016x}", self.hash()),
            None => format!("e.{:016x}.{index}", self.hash()),
        }
    }

    fn hash(self) -> u64 {
        let mut hash = FNV_OFFSET;
        hash = hash_bytes(hash, self.file.as_bytes());
        hash = hash_u32(hash, self.line);
        hash_u32(hash, self.column)
    }
}

impl Element {
    pub(super) fn with_layout(mut self, layout: LayoutSpec) -> Self {
        let render = self.render;
        self.render = Box::new(move |cx| render(cx).layout(layout));
        self
    }
}

fn compile_children(
    scope: &UiScope,
    context: &UiRenderContext,
    component_id: ComponentId,
    force_components: bool,
    children: Vec<Element>,
) -> Vec<UiElement> {
    children
        .into_iter()
        .enumerate()
        .map(|(index, child)| {
            let child_scope = UiScope::from_path(scope.path().child(child.key.segment(index)));
            child.compile_internal(&child_scope, context, component_id, force_components)
        })
        .collect()
}

fn hash_u32(hash: u64, value: u32) -> u64 {
    hash_bytes(hash, &value.to_le_bytes())
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub(super) fn stable_hash(value: &str) -> u64 {
    hash_bytes(FNV_OFFSET, value.as_bytes())
}

pub(super) fn location_hash(location: &'static Location<'static>) -> u64 {
    let mut hash = FNV_OFFSET;
    hash = hash_bytes(hash, location.file().as_bytes());
    hash = hash_u32(hash, location.line());
    hash_u32(hash, location.column())
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
