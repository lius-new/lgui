use std::any::Any;

use lgui_core::{
    application::ApplicationContext,
    memory::{CacheAdapter, CacheDomain, DomainRegistration, TrimResult},
};
use lgui_platform_win32::{CoalescedTrim, Win32Dispatcher};

pub(crate) fn install(context: &ApplicationContext, dispatcher: &Win32Dispatcher) -> Box<dyn Any> {
    register_gdi(context, dispatcher);
    lgui_render_win32_raster::backend::install_gdi_environment(context, dispatcher)
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
