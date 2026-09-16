use std::any::Any;

use lgui_core::{
    application::ApplicationContext,
    memory::{CacheAdapter, CacheDomain, DomainRegistration, TrimResult},
};
use lgui_platform_win32::{
    render_support::{blur, image},
    CoalescedTrim, Win32Dispatcher,
};

pub(crate) fn install(context: &ApplicationContext, dispatcher: &Win32Dispatcher) -> Box<dyn Any> {
    register_decoded_images(context, dispatcher);
    register_blur(context, dispatcher);
    Box::new(())
}

fn register_decoded_images(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), image::trim_decoded_image_cache);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::DecodedImage,
        context.memory().next_instance_id(),
        "application:d2d-raster-images",
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

fn register_blur(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), blur::trim_blur_caches);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::Blur,
        context.memory().next_instance_id(),
        "application:d2d-raster-blur",
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
