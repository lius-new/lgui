use std::any::Any;

use lgui_core::{
    application::ApplicationContext,
    memory::{CacheAdapter, CacheDomain, CacheUsage, DomainRegistration, TrimResult},
};
use lgui_platform_win32::{CoalescedTrim, Win32Dispatcher};

use crate::static_layer;

#[cfg(feature = "gdi")]
use crate::render_cache;
#[cfg(feature = "d2d")]
use crate::{blur, image};

#[cfg(feature = "gdi")]
pub fn install_gdi_environment(
    context: &ApplicationContext,
    dispatcher: &Win32Dispatcher,
) -> Box<dyn Any> {
    register_static_layer(context, dispatcher);
    Box::new(lgui_core::backend::install_render_cache(
        render_cache::portable_render_cache_handle(),
    ))
}

#[cfg(feature = "d2d")]
pub fn install_d2d_environment(
    context: &ApplicationContext,
    dispatcher: &Win32Dispatcher,
) -> Box<dyn Any> {
    register_decoded_images(context, dispatcher);
    register_blur(context, dispatcher);
    register_static_layer(context, dispatcher);
    Box::new(())
}

#[cfg(feature = "d2d")]
fn register_decoded_images(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), image::trim_decoded_image_cache);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::DecodedImage,
        context.memory().next_instance_id(),
        "application:win32-raster-images",
        CacheAdapter::managed(
            image::decoded_image_cache_usage,
            move |request| {
                let before = image::decoded_image_cache_usage().resident_bytes();
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher.post(move || image::set_decoded_image_cache_budget(budget));
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}

#[cfg(feature = "d2d")]
fn register_blur(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), blur::trim_blur_caches);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::Blur,
        context.memory().next_instance_id(),
        "application:win32-raster-blur",
        CacheAdapter::managed(
            blur::blur_cache_usage,
            move |request| {
                let before = blur::blur_cache_usage().resident_bytes();
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher.post(move || blur::set_blur_cache_budget(budget));
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}

fn register_static_layer(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(
        dispatcher.clone(),
        static_layer::trim_static_layer_memory_cache,
    );
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::StaticLayer,
        context.memory().next_instance_id(),
        "application:win32-raster-static-layer",
        CacheAdapter::managed(
            || {
                let stats = static_layer::static_layer_memory_cache_stats();
                CacheUsage {
                    cache_bytes: stats.bytes,
                    cpu_bytes: stats.bytes,
                    entries: stats.entry_count,
                    hits: stats.hits,
                    misses: stats.misses,
                    evictions: stats.evictions,
                    ..Default::default()
                }
            },
            move |request| {
                let before = static_layer::static_layer_memory_cache_stats().bytes;
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher
                    .post(move || static_layer::set_static_layer_memory_cache_budget(budget));
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}
