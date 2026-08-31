pub(super) fn release_visual_caches() {
    crate::core::clear_scroll_raster_command_cache();

    #[cfg(feature = "images")]
    {
        super::clear_cached_decoded_image_cache();
        super::clear_cached_image_cache();
    }
    #[cfg(feature = "svg")]
    super::svg::clear_svg_bitmap_cache();
    #[cfg(feature = "advanced-rendering")]
    {
        super::enhanced::blur::clear_blur_caches();
        super::enhanced::clear_gdi_renderer_caches();
        super::enhanced::image::clear_decoded_image_cache();
        super::enhanced::static_layer::clear_static_layer_memory_cache();
        super::enhanced::static_layer_raster_cache::clear();
    }
}

pub(super) fn trim_process_working_set() {
    unsafe {
        use windows::Win32::System::{
            ProcessStatus::EmptyWorkingSet,
            Threading::{GetCurrentProcess, SetProcessWorkingSetSize},
        };

        let process = GetCurrentProcess();
        if EmptyWorkingSet(process).is_err() {
            let _ = SetProcessWorkingSetSize(process, usize::MAX, usize::MAX);
        }
    }
}
