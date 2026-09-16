use std::{cell::RefCell, sync::Arc};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StaticLayerMemoryCacheStats {
    pub entry_count: usize,
    pub bytes: usize,
    pub budget_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub stores: u64,
    pub evictions: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StaticLayerMemoryCachePrefixStats {
    pub entry_count: usize,
    pub bytes: usize,
}

#[derive(Clone)]
pub struct RenderCacheHandle {
    stats: Arc<dyn Fn() -> StaticLayerMemoryCacheStats + Send + Sync>,
    prefix_stats: Arc<dyn Fn(&str) -> StaticLayerMemoryCachePrefixStats + Send + Sync>,
    prefix_entry_ids: Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>,
}

impl RenderCacheHandle {
    pub fn new(
        stats: impl Fn() -> StaticLayerMemoryCacheStats + Send + Sync + 'static,
        prefix_stats: impl Fn(&str) -> StaticLayerMemoryCachePrefixStats + Send + Sync + 'static,
        prefix_entry_ids: impl Fn(&str) -> Vec<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            stats: Arc::new(stats),
            prefix_stats: Arc::new(prefix_stats),
            prefix_entry_ids: Arc::new(prefix_entry_ids),
        }
    }
}

thread_local! {
    static RENDER_CACHE: RefCell<Option<RenderCacheHandle>> = const { RefCell::new(None) };
}

#[cfg(any(test, all(target_os = "windows", feature = "renderer-gdi")))]
pub(crate) struct RenderCacheGuard {
    previous: Option<RenderCacheHandle>,
}

#[cfg(any(test, all(target_os = "windows", feature = "renderer-gdi")))]
impl Drop for RenderCacheGuard {
    fn drop(&mut self) {
        RENDER_CACHE.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

#[cfg(any(test, all(target_os = "windows", feature = "renderer-gdi")))]
pub(crate) fn install_render_cache(handle: RenderCacheHandle) -> RenderCacheGuard {
    let previous = RENDER_CACHE.with(|current| current.borrow_mut().replace(handle));
    RenderCacheGuard { previous }
}

pub fn static_layer_cache_stats() -> StaticLayerMemoryCacheStats {
    RENDER_CACHE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or_else(StaticLayerMemoryCacheStats::default, |cache| {
                (cache.stats)()
            })
    })
}

pub fn static_layer_cache_stats_for_prefix(prefix: &str) -> StaticLayerMemoryCachePrefixStats {
    RENDER_CACHE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or_else(StaticLayerMemoryCachePrefixStats::default, |cache| {
                (cache.prefix_stats)(prefix)
            })
    })
}

pub fn static_layer_cache_entry_ids_for_prefix(prefix: &str) -> Vec<String> {
    RENDER_CACHE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |cache| (cache.prefix_entry_ids)(prefix))
    })
}

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
