use super::*;

fn bitmap() -> StaticLayerBitmap {
    StaticLayerBitmap {
        width: 1,
        height: 2,
        pixels: vec![0; 8],
    }
}

#[test]
fn raster_policy_evicts_shorter_retention_then_lower_priority() {
    let mut cache = StaticLayerMemoryCache::default();
    cache.governor_budget_bytes = 64;
    for (key, retention, priority) in [
        (
            "frame-high",
            crate::memory::RetentionClass::Frame,
            crate::memory::CachePriority::High,
        ),
        (
            "scene-low",
            crate::memory::RetentionClass::Scene,
            crate::memory::CachePriority::Low,
        ),
        (
            "scene-high",
            crate::memory::RetentionClass::Scene,
            crate::memory::CachePriority::High,
        ),
        (
            "session-low",
            crate::memory::RetentionClass::Session,
            crate::memory::CachePriority::Low,
        ),
    ] {
        assert!(cache.store(
            key,
            key.to_owned(),
            bitmap(),
            64,
            RasterCachePolicy::memory(retention, priority),
        ));
    }

    cache.budget_bytes = 16;
    cache.evict_to_budget();

    assert!(!cache.entries.contains_key("frame-high"));
    assert!(!cache.entries.contains_key("scene-low"));
    assert!(cache.entries.contains_key("scene-high"));
    assert!(cache.entries.contains_key("session-low"));
}
