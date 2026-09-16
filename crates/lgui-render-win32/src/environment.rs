use std::any::Any;

use lgui_core::{
    application::ApplicationContext,
    memory::{CacheAdapter, CacheDomain, CacheUsage, DomainRegistration, TrimResult},
};
use lgui_platform_win32::{CoalescedTrim, Win32Dispatcher};

use crate::enhanced;

#[cfg(feature = "advanced-rendering")]
pub(crate) fn install_gdi(
    context: &ApplicationContext,
    dispatcher: &Win32Dispatcher,
) -> Box<dyn Any> {
    register_static_layer(context, dispatcher);
    register_gdi(context, dispatcher);
    Box::new(lgui_core::backend::install_render_cache(
        enhanced::portable_render_cache_handle(),
    ))
}

#[cfg(feature = "d2d")]
pub(crate) fn install_d2d(
    context: &ApplicationContext,
    dispatcher: &Win32Dispatcher,
) -> Box<dyn Any> {
    register_decoded_images(context, dispatcher);
    register_blur(context, dispatcher);
    register_static_layer(context, dispatcher);
    #[cfg(feature = "advanced-rendering")]
    {
        return Box::new(lgui_core::backend::install_render_cache(
            enhanced::portable_render_cache_handle(),
        ));
    }
    #[cfg(not(feature = "advanced-rendering"))]
    Box::new(())
}

#[cfg(feature = "d2d")]
fn register_decoded_images(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(
        dispatcher.clone(),
        enhanced::image::trim_decoded_image_cache,
    );
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::DecodedImage,
        context.memory().next_instance_id(),
        "application:win32-enhanced-images",
        CacheAdapter::managed(
            enhanced::image::decoded_image_cache_usage,
            move |request| {
                let before = enhanced::image::decoded_image_cache_usage().resident_bytes();
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher.post(move || {
                    enhanced::image::set_decoded_image_cache_budget(budget);
                });
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}

#[cfg(feature = "d2d")]
fn register_blur(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), enhanced::blur::trim_blur_caches);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::Blur,
        context.memory().next_instance_id(),
        "application:win32-blur",
        CacheAdapter::managed(
            enhanced::blur::blur_cache_usage,
            move |request| {
                let before = enhanced::blur::blur_cache_usage().resident_bytes();
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher.post(move || {
                    enhanced::blur::set_blur_cache_budget(budget);
                });
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}

fn register_static_layer(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(
        dispatcher.clone(),
        enhanced::static_layer::trim_static_layer_memory_cache,
    );
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::StaticLayer,
        context.memory().next_instance_id(),
        "application:win32-static-layer",
        CacheAdapter::managed(
            || {
                let stats = enhanced::static_layer::static_layer_memory_cache_stats();
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
                let before = enhanced::static_layer::static_layer_memory_cache_stats().bytes;
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher.post(move || {
                    enhanced::static_layer::set_static_layer_memory_cache_budget(budget);
                });
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}

#[cfg(feature = "advanced-rendering")]
fn register_gdi(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), enhanced::trim_gdi_renderer_caches);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::Gdi,
        context.memory().next_instance_id(),
        "application:win32-gdi-shared",
        CacheAdapter::managed(
            enhanced::gdi_renderer_cache_usage,
            move |request| {
                let before = enhanced::gdi_renderer_cache_usage().resident_bytes();
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher.post(move || {
                    enhanced::set_gdi_renderer_cache_budget(budget);
                });
            },
        ),
    ));
    lgui_core::backend::retain_memory_registration(context, registration);
}
