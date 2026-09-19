//! Internal service-provider interface used by LGUI backend crates.
//!
//! This module is public so separately published backend crates can integrate
//! with the core runtime. Applications should use the higher-level APIs.

use std::sync::Arc;

use crate::{
    application::RenderErrorStage,
    application::{AppView, ApplicationContext, RenderError},
    core::UiTaskSpawner,
    text::TextSystemHandle,
    window::{WindowId, WindowManager},
};

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

pub struct TextEnvironment {
    _fonts: FontEnvironment,
    _assets: crate::text::FontAssetsGuard,
}

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

pub fn font_families() -> &'static [&'static str] {
    crate::text::font_families()
}

pub fn font_assets() -> Arc<Vec<crate::text::FontAsset>> {
    crate::text::font_assets()
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

pub struct RenderCacheEnvironment {
    _guard: crate::renderer::RenderCacheGuard,
}

pub fn install_render_cache(cache: crate::renderer::RenderCacheHandle) -> RenderCacheEnvironment {
    RenderCacheEnvironment {
        _guard: crate::renderer::install_render_cache(cache),
    }
}

#[cfg(feature = "raster-effects")]
pub fn composite_shadow(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    style: crate::core::ShadowStyle,
) {
    crate::renderer::shadow::composite_shadow(pixels, width, height, style);
}

pub fn session_suspend_rendering(session: &mut UiSession) {
    session.suspend_rendering();
}

pub fn session_trim_component_outputs(session: &mut UiSession, target_bytes: usize) -> usize {
    session.trim_component_outputs(target_bytes)
}

pub fn session_trim_host_scene(session: &mut UiSession) -> usize {
    session.trim_host_scene()
}

pub fn session_memory_usage(session: &UiSession) -> (CacheUsage, CacheUsage) {
    session.memory_usage()
}
