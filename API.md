# API Direction

## Current Migration Surface

The first extraction preserves the existing public runtime names so the application can move
without combining a physical crate migration with an API rewrite:

- `RenderCx`, `Element`, `RootComponent`, and declarative content
- `use_state`, `use_effect`, `use_context`, `use_observable`, and generic router context
- `UiRuntime`, `HostTree`, `LayoutRuntime`, `HostRuntime`, and `UiSession`
- generic input events, event handlers, scene data, scale, and geometry

Application behavior is added to `UiEventContext` with an application-owned extension trait. The
GUI crate does not know application routes, stores, themes, network runtimes, or window commands.

## Intended Ergonomic Surface

After the physical extraction is stable, state should move from a value/setter tuple toward a
single typed handle:

```rust,ignore
fn counter(cx: &mut ViewCx) -> impl IntoElement {
    let count = cx.state(0_i32);

    vstack((
        text(count.get()),
        button("one up").on_click(move || count.update(|value| *value += 1)),
    ))
}
```

This example is a design target, not the current `0.1.0` API. Store, router, diagnostics, widgets,
platform, and renderer features are added only when their implementations cross into this crate.
