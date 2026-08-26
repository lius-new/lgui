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

## Application And Platform

`Application<B>` owns backend-neutral window options and an `AppView`. Backends implement
`ApplicationBackend`, so application code does not pass native handles into the component tree:

```rust,ignore
Application::with_backend(backend)
    .window_options(WindowOptions::new("Counter", Size::new(640, 480)))
    .run(app)?;
```

The portable platform surface includes `InputEvent`, `InputSink`, `WakeHandle`, executor-neutral
tasks, `Clipboard`, and pure DPI scale resolution. The optional `backend-win32` feature adds the
Win32 clipboard, monitor/DPI adapter, layered window host, and layered GDI backbuffer. Native
window and device-context types stay inside `platform::win32`; the portable core never exposes
them.

The layered host accepts application callbacks for scene drawing, task/session setup, and error
reporting. This keeps image/font registries, the network executor, and application logging outside
the platform crate.

## Renderer And Presenter

`RenderBackend<Target>` is the scene drawing contract. `UiPresenter<Target, State>` owns frame,
input, animation, and invalidation coordination, while `PresenterPlugin<State>` lets an application
attach typed state synchronization without teaching `lgui` about its runtime. `PresentRequest`,
`PresentMode`, and `PresentStats` are shared by the GDI and Direct2D adapters.

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
on Tokio.

## Store

The default `store` feature provides typed definitions, selectors, actions, and an injected
runtime. Store creation and mutation remain independent from application globals:

```rust,ignore
struct CounterStore {
    count: i32,
}

impl CounterStore {
    fn inc(&mut self) {
        self.count += 1;
    }
}

const COUNTER: StoreDefinition<CounterStore> =
    create("counter", || CounterStore { count: 1 });
const INC: StoreAction<CounterStore> = COUNTER.action(CounterStore::inc);

fn use_counter(cx: &mut RenderCx<'_, '_>) -> (i32, BoundStoreAction<CounterStore>) {
    (COUNTER.select(cx, |store| store.count), INC.bind(cx))
}
```

The application provides `StoreContext` above component consumers. A selector subscribes to one
typed store and invalidates its component only when the selected value changes. Multiple store
notifications before the next frame are batched by the component update queue. Mutation code does
not declare string paths, and an unselected field cannot refresh the component.

Platform-specific implementations are feature-gated; portable contracts remain available without
default features.

## Router

The default `router` feature provides a generic route runtime with typed history and subscriptions:

```rust,ignore
#[derive(Clone, PartialEq)]
enum Route {
    Home,
    Settings,
}

let router = Router::new(Route::Home);
router.navigate(Route::Settings);
router.replace(Route::Home);
router.back();
```

`Router<R>` owns only generic route values. `navigate` pushes history, `replace` preserves the
existing history entry, and `back` restores the most recent distinct route. No-op transitions do
not notify subscribers. `RenderCx::use_router` returns a directly bound `RouterContext`; an
application adapter can instead consume `use_router_snapshot` and bind Context actions through its
own lifecycle boundary.

Components below a `RouterContext<R>` can call `use_route`, `use_navigate`, `use_replace`, or
`use_back`. Route notifications invalidate the component that called the Router hook, while
unrelated sibling components remain clean.

## Theme and widgets

The default `widgets` feature enables the `theme` feature and currently exports the controlled
`switch`, `slider`, and `select` controls. Their values always come from the caller, and their
callbacks report the proposed next value; widgets do not read Stores or application globals.

```rust,ignore
let theme = ThemeContext::new(ThemeTokens {
    colors: ColorTokens {
        accent: Color(0x2FB8C5),
        ..ColorTokens::default()
    },
    ..ThemeTokens::default()
});

context_provider(
    theme,
    switch(rect, enabled, move |next| set_enabled(next)),
)
```

Default widget styles resolve the nearest `ThemeContext` during element rendering. An explicit
`.style(...)` overrides those tokens. `ThemeTokens` separates semantic colors, spacing, and
typography from application theme schemas, so applications map their own palette into this small
public contract.

## Diagnostics

The optional `diagnostics` feature provides backend-neutral frame metrics, snapshots, bounded
collection, recent-sample queries, and provider/sink contracts. Renderers and platform adapters
produce `FrameSample` values without exposing their native handles through the public model:

```rust,ignore
let mut frames = FrameCollector::new(120);
frames.record(sample);

let snapshot = frames.snapshot();
let recent = frames.query(DiagnosticsQuery::recent(30));
```

Enable `diagnostics-serde` when an application needs to serialize metric value types. HUDs,
operating-system resource sampling, tree inspection, debug commands, and application cache
operations remain application concerns.
