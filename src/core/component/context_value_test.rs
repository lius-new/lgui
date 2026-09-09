use super::*;
use crate::core::UiId;

#[test]
fn nested_provider_restores_outer_value() {
    let components = ComponentTree::new();
    components.begin_render();
    let outer = components.root(UiId::owned("outer"), "outer");
    let inner = components.positioned_child(outer, 1, 0, "inner");
    let consumer = components.positioned_child(inner, 2, 0, "consumer");
    let contexts = ContextRegistry::new();
    contexts.begin_render();

    let outer_guard = contexts.provide(outer, 1_u32, &components);
    assert_eq!(contexts.read::<u32>(consumer), Some(1));
    {
        let _inner_guard = contexts.provide(inner, 2_u32, &components);
        assert_eq!(contexts.read::<u32>(consumer), Some(2));
    }
    assert_eq!(contexts.read::<u32>(consumer), Some(1));
    drop(outer_guard);
}

#[test]
fn changed_provider_invalidates_only_recorded_consumers() {
    let components = ComponentTree::new();
    let contexts = ContextRegistry::new();

    components.begin_render();
    contexts.begin_render();
    let provider = components.root(UiId::owned("provider"), "provider");
    let consumer = components.positioned_child(provider, 1, 0, "consumer");
    let unrelated = components.positioned_child(provider, 1, 1, "unrelated");
    {
        let _guard = contexts.provide(provider, 1_u32, &components);
        assert_eq!(contexts.read::<u32>(consumer), Some(1));
    }
    components.finish_component(consumer);
    components.finish_component(unrelated);
    components.finish_component(provider);
    components.end_render();
    contexts.end_render(&components);
    assert!(!components.is_dirty(consumer));
    assert!(!components.is_dirty(unrelated));

    components.begin_render();
    contexts.begin_render();
    let provider = components.root(UiId::owned("provider"), "provider");
    let _guard = contexts.provide(provider, 2_u32, &components);

    assert!(components.is_dirty(consumer));
    assert!(!components.is_dirty(unrelated));
}

#[test]
fn abort_render_restores_provider_values_and_consumer_subscriptions() {
    let components = ComponentTree::new();
    let contexts = ContextRegistry::new();

    components.begin_render();
    contexts.begin_render();
    let provider = components.root(UiId::owned("provider"), "provider");
    let consumer = components.positioned_child(provider, 1, 0, "consumer");
    {
        let _guard = contexts.provide(provider, 1_u32, &components);
        assert_eq!(contexts.read::<u32>(consumer), Some(1));
    }
    components.finish_component(consumer);
    components.finish_component(provider);
    components.end_render();
    contexts.end_render(&components);

    components.begin_render();
    contexts.begin_render();
    let provider = components.root(UiId::owned("provider"), "provider");
    {
        let _guard = contexts.provide(provider, 2_u32, &components);
    }
    contexts.abort_render(&components);
    components.abort_render();

    components.begin_render();
    contexts.begin_render();
    let provider = components.root(UiId::owned("provider"), "provider");
    let consumer = components.positioned_child(provider, 1, 0, "consumer");
    {
        let _guard = contexts.provide(provider, 1_u32, &components);
        assert_eq!(contexts.read::<u32>(consumer), Some(1));
    }
    assert!(!components.is_dirty(consumer));
}
