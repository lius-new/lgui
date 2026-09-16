use std::hash::{Hash, Hasher};
#[cfg(feature = "svg")]
use std::sync::Arc;

#[cfg(feature = "backend-winit")]
use lgui_core::memory::MemoryGovernor;
use lgui_core::{
    application::ApplicationContext,
    core::{ImageRequest, ScenePrimitive, UiImageSource},
    memory::DomainInstanceId,
};

#[cfg(feature = "renderer-skia")]
use crate::AssetBytes;
#[cfg(feature = "backend-winit")]
use crate::RemoteImageLoaderHandle;
use crate::{cache, resources, ImageCacheHandle, RenderResources};

pub fn render_resources() -> RenderResources {
    resources::render_resources()
}

#[cfg(feature = "renderer-skia")]
pub fn cached_image_bytes(request: &ImageRequest) -> Option<AssetBytes> {
    cache::cached_image_bytes(request)
}

#[cfg(feature = "backend-winit")]
pub fn async_image_cache(
    loader: RemoteImageLoaderHandle,
    wake: impl Fn() + Send + Sync + 'static,
    budget_bytes: usize,
    governor: MemoryGovernor,
) -> ImageCacheHandle {
    cache::async_image_cache(loader, wake, budget_bytes, governor)
}

pub struct ImageCacheEnvironment {
    _guard: cache::ImageCacheGuard,
}

pub fn install_image_cache(cache: ImageCacheHandle) -> ImageCacheEnvironment {
    ImageCacheEnvironment {
        _guard: crate::cache::install_image_cache(cache),
    }
}

pub fn update_image_reachability(owner: DomainInstanceId, requests: &[ImageRequest]) {
    cache::update_image_reachability(owner, requests);
}

pub fn with_render_resources<R>(
    context: &ApplicationContext,
    render_resources: RenderResources,
    render: impl FnOnce() -> R,
) -> R {
    #[cfg(feature = "svg")]
    {
        let icons = context
            .try_resource::<crate::icons::IconRegistration>()
            .map(|registration| Arc::clone(&registration.0));
        return crate::icons::with_icon_registry(icons, || {
            resources::with_render_resources(render_resources, render)
        });
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = context;
        resources::with_render_resources(render_resources, render)
    }
}

pub fn with_render_resources_unscoped<R>(
    render_resources: RenderResources,
    render: impl FnOnce() -> R,
) -> R {
    resources::with_render_resources(render_resources, render)
}

#[cfg(feature = "svg")]
pub fn resolve_svg(key: &str) -> Option<std::borrow::Cow<'static, str>> {
    crate::icons::resolve_svg(key)
}

#[cfg(feature = "svg")]
pub fn configured_svg_icons(context: &ApplicationContext) -> Option<crate::icons::SvgIconRegistry> {
    context
        .try_resource::<crate::icons::IconRegistration>()
        .map(|registration| (*registration.0).clone())
}

pub fn resolved_content_signature(commands: &[ScenePrimitive], signature: u64) -> u64 {
    fn visit(
        commands: &[ScenePrimitive],
        state: &mut Option<std::collections::hash_map::DefaultHasher>,
        signature: u64,
    ) {
        for command in commands {
            match command {
                ScenePrimitive::Image {
                    request,
                    source: UiImageSource::Url(_) | UiImageSource::File(_),
                    ..
                } => {
                    let hasher = state.get_or_insert_with(|| {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        signature.hash(&mut hasher);
                        hasher
                    });
                    std::mem::discriminant(&crate::request_image(request)).hash(hasher);
                }
                ScenePrimitive::CompositingLayer { commands, .. }
                | ScenePrimitive::StaticLayer { commands, .. }
                | ScenePrimitive::ScrollRaster { commands, .. }
                | ScenePrimitive::Clip { commands, .. }
                | ScenePrimitive::ClipPath { commands, .. } => visit(commands, state, signature),
                _ => {}
            }
        }
    }

    let mut state = None;
    visit(commands, &mut state, signature);
    state.map_or(signature, |hasher| hasher.finish())
}
