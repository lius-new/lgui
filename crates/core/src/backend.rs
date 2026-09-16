//! Internal service-provider interface used by LGUI backend crates.
//!
//! This module is public so separately published backend crates can integrate
//! with the core runtime. Applications should use the higher-level APIs.

#[cfg(feature = "svg")]
use std::borrow::Cow;
#[cfg(any(feature = "renderer-skia", feature = "svg"))]
use std::sync::Arc;

use crate::{
    application::RenderErrorStage,
    application::{AppView, ApplicationContext, RenderError},
    core::UiTaskSpawner,
    memory::CacheRegistration,
    text::TextSystemHandle,
    window::{WindowId, WindowManager},
};

#[cfg(any(
    feature = "backend-winit",
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    all(feature = "backend-win32", feature = "renderer-skia")
))]
use crate::{memory::CacheUsage, session::UiSession};

pub use crate::window::WindowCommand;

pub fn application_root_view(context: ApplicationContext, view: AppView) -> AppView {
    crate::application::application_root_view(context, view)
}

pub fn install_window_commands(
    windows: &WindowManager,
    handler: impl Fn(WindowCommand) + Send + Sync + 'static,
) {
    windows.install(handler);
}

pub fn render_error(
    window: WindowId,
    renderer: &'static str,
    stage: RenderErrorStage,
    operation: &'static str,
    code: i32,
    message: impl Into<String>,
) -> RenderError {
    RenderError::new(window, renderer, stage, operation, code, message)
}

pub fn report_render_error(context: &ApplicationContext, error: RenderError) {
    context.report_render_error(error);
}

pub fn task_spawner(context: &ApplicationContext) -> Option<UiTaskSpawner> {
    context.task_spawner()
}

pub fn retain_memory_registration(context: &ApplicationContext, registration: CacheRegistration) {
    context.retain_memory_registration(registration);
}

pub struct FontEnvironment {
    _families: crate::text::FontFamiliesGuard,
    _system: crate::text::TextSystemGuard,
}

pub fn configured_font_families(context: &ApplicationContext) -> &'static [&'static str] {
    context
        .try_resource::<crate::text::FontFamilies>()
        .map_or(&["Segoe UI"][..], |families| families.0)
}

pub fn install_font_environment(
    context: &ApplicationContext,
    system: TextSystemHandle,
) -> FontEnvironment {
    FontEnvironment {
        _families: crate::text::install_font_families(configured_font_families(context)),
        _system: crate::text::install_text_system(system),
    }
}

#[cfg(feature = "renderer-skia")]
pub struct TextEnvironment {
    _fonts: FontEnvironment,
    _assets: crate::text::FontAssetsGuard,
}

#[cfg(feature = "renderer-skia")]
pub fn install_text_environment(
    context: &ApplicationContext,
    system: TextSystemHandle,
) -> TextEnvironment {
    let assets = context
        .try_resource::<crate::text::FontAssets>()
        .map_or_else(Default::default, |assets| Arc::clone(&assets.0));
    TextEnvironment {
        _fonts: install_font_environment(context, system),
        _assets: crate::text::install_font_assets(assets),
    }
}

#[cfg(feature = "renderer-skia")]
pub fn font_families() -> &'static [&'static str] {
    crate::text::font_families()
}

#[cfg(feature = "renderer-skia")]
pub fn font_assets() -> Arc<Vec<crate::text::FontAsset>> {
    crate::text::font_assets()
}

#[cfg(feature = "images")]
pub fn render_resources() -> crate::assets::RenderResources {
    crate::assets::render_resources()
}

#[cfg(all(feature = "images", feature = "renderer-skia"))]
pub fn cached_image_bytes(
    request: &crate::core::ImageRequest,
) -> Option<crate::assets::AssetBytes> {
    crate::assets::cached_image_bytes(request)
}

#[cfg(all(feature = "images", feature = "backend-winit"))]
pub fn async_image_cache(
    loader: crate::assets::RemoteImageLoaderHandle,
    wake: impl Fn() + Send + Sync + 'static,
    budget_bytes: usize,
    governor: crate::memory::MemoryGovernor,
) -> crate::assets::ImageCacheHandle {
    crate::assets::async_image_cache(loader, wake, budget_bytes, governor)
}

#[cfg(feature = "images")]
pub struct ImageCacheEnvironment {
    _guard: crate::assets::ImageCacheGuard,
}

#[cfg(feature = "images")]
pub fn install_image_cache(cache: crate::assets::ImageCacheHandle) -> ImageCacheEnvironment {
    ImageCacheEnvironment {
        _guard: crate::assets::install_image_cache(cache),
    }
}

#[cfg(feature = "images")]
pub fn update_image_reachability(
    owner: crate::memory::DomainInstanceId,
    requests: &[crate::core::ImageRequest],
) {
    crate::assets::update_image_reachability(owner, requests);
}

#[cfg(feature = "images")]
pub fn with_render_resources<R>(
    context: &ApplicationContext,
    resources: crate::assets::RenderResources,
    render: impl FnOnce() -> R,
) -> R {
    #[cfg(feature = "svg")]
    {
        let icons = context
            .try_resource::<crate::icons::IconRegistration>()
            .map(|registration| Arc::clone(&registration.0));
        return crate::icons::with_icon_registry(icons, || {
            crate::assets::with_render_resources(resources, render)
        });
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = context;
        crate::assets::with_render_resources(resources, render)
    }
}

#[cfg(feature = "images")]
pub fn with_render_resources_unscoped<R>(
    resources: crate::assets::RenderResources,
    render: impl FnOnce() -> R,
) -> R {
    crate::assets::with_render_resources(resources, render)
}

#[cfg(feature = "svg")]
pub fn resolve_svg(key: &str) -> Option<Cow<'static, str>> {
    crate::icons::resolve_svg(key)
}

pub fn compositing_shadow(
    spec: impl std::borrow::Borrow<crate::core::CompositingLayerSpec>,
) -> Option<crate::core::ShadowStyle> {
    spec.borrow().shadow
}

#[cfg(feature = "accessibility")]
pub fn full_semantic_update(
    tree: &crate::core::HostTree,
    focus: Option<crate::core::UiId>,
) -> crate::core::SemanticUpdate {
    crate::core::SemanticUpdate::full_from_tree(tree, focus)
}

pub fn memory_begin_frame_budget_check(memory: &crate::memory::MemoryGovernor) -> bool {
    memory.begin_frame_budget_check()
}

pub fn memory_finish_frame_budget_check(memory: &crate::memory::MemoryGovernor) {
    memory.finish_frame_budget_check();
}

#[cfg(all(target_os = "windows", feature = "advanced-rendering"))]
pub struct RenderCacheEnvironment {
    _guard: crate::renderer::RenderCacheGuard,
}

#[cfg(all(target_os = "windows", feature = "advanced-rendering"))]
pub fn install_render_cache(cache: crate::renderer::RenderCacheHandle) -> RenderCacheEnvironment {
    RenderCacheEnvironment {
        _guard: crate::renderer::install_render_cache(cache),
    }
}

#[cfg(feature = "svg")]
pub fn configured_svg_icons(context: &ApplicationContext) -> Option<crate::icons::SvgIconRegistry> {
    context
        .try_resource::<crate::icons::IconRegistration>()
        .map(|registration| (*registration.0).clone())
}

#[cfg(any(
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    feature = "renderer-skia"
))]
pub fn resolved_content_signature(commands: &[crate::core::ScenePrimitive], signature: u64) -> u64 {
    crate::renderer::shadow::resolved_content_signature(commands, signature)
}

#[cfg(any(
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    feature = "renderer-skia"
))]
pub fn composite_shadow(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    style: crate::core::ShadowStyle,
) {
    crate::renderer::shadow::composite_shadow(pixels, width, height, style);
}

#[cfg(any(
    feature = "backend-winit",
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    all(feature = "backend-win32", feature = "renderer-skia")
))]
pub fn session_suspend_rendering(session: &mut UiSession) {
    session.suspend_rendering();
}

#[cfg(any(
    feature = "backend-winit",
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    all(feature = "backend-win32", feature = "renderer-skia")
))]
pub fn session_trim_component_outputs(session: &mut UiSession, target_bytes: usize) -> usize {
    session.trim_component_outputs(target_bytes)
}

#[cfg(any(
    feature = "backend-winit",
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    all(feature = "backend-win32", feature = "renderer-skia")
))]
pub fn session_trim_host_scene(session: &mut UiSession) -> usize {
    session.trim_host_scene()
}

#[cfg(any(
    feature = "backend-winit",
    feature = "renderer-gdi",
    feature = "renderer-d2d",
    all(feature = "backend-win32", feature = "renderer-skia")
))]
pub fn session_memory_usage(session: &UiSession) -> (CacheUsage, CacheUsage) {
    session.memory_usage()
}
