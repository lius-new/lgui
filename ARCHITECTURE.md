# LGUI Architecture

`lgui` is the application-independent GUI runtime extracted from the Liuguang Windows
client. Version `0.1.x` is an unstable migration series and does not promise API compatibility.
The preserved behavior and verification baseline are recorded in [`BASELINE.md`](BASELINE.md).

## Dependency Direction

Dependencies only flow in this direction:

```text
application -> public facade -> component runtime -> host runtime -> platform/backend
```

The crate must not contain application pages, domain stores, backend DTOs, application routes,
application themes, or platform handles in the backend-independent runtime.

## Current Public Surface

The runtime exposes `RenderCx`, `Element`, typed `State<T>`, Effect, Context, Observable, generic
router, retained host tree, layout, damage, and `UiSession` semantics. The tuple-based `use_state`
surface remains as a migration compatibility API for Liuguang callers.

Application-specific behavior is attached to `UiEventContext` through traits defined in the
application crate. The GUI crate exposes only propagation, scheduling flags, application context
access, and an opaque host-command slot. A small set of `#[doc(hidden)]` component-builder methods
remains public while widgets still live in the application crate; they return to crate-private
visibility when the widgets migration is complete.

Async effects are available with the `async` feature through the executor-neutral `UiExecutor`
contract and standard `Future` cancellation. The `tokio` feature only exports a Tokio executor
adapter; the component runtime has no Tokio-specific scheduling or cancellation code.

The `store` feature owns the typed Store registry, runtime, selectors, and bound actions.
Applications inject an `Arc<StoreRuntime>` through `StoreContext`; Store hooks never resolve an
application singleton. Notifications are store-typed, while selector equality controls component
invalidation, so business mutation code does not expose or construct string paths.

The `router` feature owns generic `Router<R>` history, route subscriptions, and Router hooks.
Route values remain application-defined. Applications may bind the generic `RouterContext`
callbacks through an adapter when navigation must also publish business lifecycle events or clear
platform caches; those concerns do not enter `lgui`.

The `theme` feature defines semantic color, spacing, and typography tokens plus `ThemeContext`.
The `widgets` feature contains only controlled, Store-agnostic controls. Widget defaults resolve
tokens from Context at element render time; explicit styles remain available. Application theme
catalogs, persistence, branded palette fields, and business callbacks stay in the application.

The `diagnostics` feature owns backend-neutral frame metric values, snapshots, bounded collection,
recent-sample queries, and provider/sink contracts. `diagnostics-serde` adds serialization for
metric value types without making serialization part of the portable core. Platform resource
sampling, HUD rendering, tree inspection, debug commands, `AppRuntime`, and cache mutation remain
in the application adapter.

`Application<B>` and `ApplicationBackend` define window startup without choosing a platform in the
portable layer. `RenderBackend`, `UiPresenter`, and `PresenterPlugin` define rendering and frame
coordination without concrete device handles. Input, wake, task, clipboard, and scale contracts
are platform-neutral.

The optional `backend-win32` feature is isolated under `platform::win32`. It owns Win32 clipboard
access, monitor/DPI queries, the layered auxiliary window host, and its GDI backbuffer. The host
receives scene drawing, session configuration, and error reporting from the application, so it has
no dependency on Liuguang configuration, resources, network executors, logging, or `AppRuntime`.
The main Liuguang window loop and concrete resource-heavy GDI/Direct2D drawing remain application
adapters until their image, font, icon, and custom-paint registries have public contracts.

## Forbidden Dependencies

The `core`, `frame`, `host`, and `session` modules must not depend on:

- `native/windows` or `crate::frontend`
- Win32, GDI, Direct2D, or concrete renderer types
- pages, application runtime, business stores, application routes, components, or themes

These boundaries are checked by `tests/architecture.rs`: portable sources reject Windows and
application dependencies, while `platform::win32` separately rejects application dependencies.
The portable dependency boundary is also checked by compiling with no default features.
