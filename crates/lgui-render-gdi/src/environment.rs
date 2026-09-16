use std::any::Any;

use lgui_core::{
    application::ApplicationContext,
    memory::{CacheAdapter, CacheDomain, CacheUsage, DomainRegistration, TrimResult},
};
use lgui_platform_win32::{CoalescedTrim, Win32Dispatcher};

use crate::{render_cache, static_layer};

pub(crate) fn install(context: &ApplicationContext, dispatcher: &Win32Dispatcher) -> Box<dyn Any> {
    register_gdi(context, dispatcher);
    register_static_layer(context, dispatcher);
    Box::new(lgui_core::backend::install_render_cache(
        render_cache::portable_render_cache_handle(),
    ))
}

fn register_gdi(context: &ApplicationContext, dispatcher: &Win32Dispatcher) {
    let trim = CoalescedTrim::new(dispatcher.clone(), crate::backend::trim_gdi_renderer_caches);
    let budget_dispatcher = dispatcher.clone();
    let registration = context.memory().register(DomainRegistration::new(
        CacheDomain::Gdi,
        context.memory().next_instance_id(),
        "application:win32-gdi",
        CacheAdapter::managed(
            crate::backend::gdi_renderer_cache_usage,
            move |request| {
                let before = crate::backend::gdi_renderer_cache_usage().resident_bytes();
                trim.request(request.target_bytes);
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
            move |budget| {
                budget_dispatcher
                    .post(move || crate::backend::set_gdi_renderer_cache_budget(budget));
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
        "application:gdi-static-layer",
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
