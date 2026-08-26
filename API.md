# API Direction

## Current Migration Surface

The first extraction preserves the existing public runtime names so the application can move
without combining a physical crate migration with an API rewrite:

- `RenderCx`, `Element`, `RootComponent`, and declarative content
- `State<T>`, compatibility `use_state`, Effects, Context, Observable, and generic router context
- `UiRuntime`, `HostTree`, `LayoutRuntime`, `HostRuntime`, and `UiSession`
- generic input events, event handlers, scene data, scale, and geometry

Application behavior is added to `UiEventContext` with an application-owned extension trait. The
GUI crate does not know application routes, stores, themes, network runtimes, or window commands.

## State

New components use one typed handle. `update` receives the latest value, so event handlers do not
capture a render snapshot:

```rust,ignore
fn counter(cx: &mut RenderCx<'_, '_>) -> Element {
    let count = cx.state(0_i32);

    vstack((
        text(count.get()),
        button("one up").on_click(move || count.update(|value| *value += 1)),
    ))
}
```

`State::set`, `State::update`, and `State::try_update` update the shared hook cell and enqueue only
that component boundary. Multiple updates before the next frame are batched into one dirty ID.
Handles from an unmounted component generation cannot invalidate a later generation. Existing
Liuguang code may continue using `use_state` and `StateSetter` while it migrates.

## Effect And Async Boundaries

Effects are staged during rendering and run only when the platform reports a successful present.
Dependency changes and unmounts run the previous cleanup before the next committed effect.

The `async` feature adds cancellable async Effects against the executor-neutral `UiExecutor`
contract. The `tokio` feature adds `TokioExecutor`; component code and cancellation do not depend
on Tokio. Store, diagnostics, widgets, platform, and renderer features are added only when their
implementations cross into this crate.
