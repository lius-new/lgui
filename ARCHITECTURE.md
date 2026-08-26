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
access, and an opaque host-command slot. Application-owned advanced components use the same public
element and event contracts as the widgets shipped by `lgui`; they do not require a dispatcher or
presenter facade in `frontend`.

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
The `widgets` feature contains application-neutral display/layout primitives and controlled,
Store-agnostic controls. It currently owns `button`, `panel`, `stack`, `text`, `divider`,
`faded_divider`, `checkbox`, `switch`, `slider`, and `select`. Widget defaults resolve tokens from
Context at element render time; explicit styles remain available. Application theme catalogs,
persistence, branded palette fields, async business dispatch, and business callbacks stay in the
application.

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
access, monitor/DPI queries, the layered auxiliary window host, and its GDI backbuffer. The default
`renderer-gdi` feature builds on that boundary with a runnable `Win32Application` message loop and
a basic GDI renderer for geometry, text, clipping, and retained raster subcommands. The backend
owns native window creation, DPI/input translation, wake-driven repainting, and successful-present
Effect commits without exposing native handles to the component tree.

The layered host still receives scene drawing, session configuration, and error reporting from
the application, so it has no dependency on Liuguang configuration, resources, network executors,
logging, or `AppRuntime`. Liuguang's resource-heavy GDI/Direct2D drawing remains an application
adapter for images, SVG, blur, fonts/icons, and custom paint alongside the neutral
`RenderResources` contracts available to public backends. Its cache, remote-fetch, and
brand-resource policies intentionally do not enter the portable crate. The standalone counter
target and independent `lgui-showcase` package exercise
the public Win32 backend without a Liuguang dependency. The showcase's manifest has no dependency
other than `lgui`, and covers State, committed Effects, typed Store selection/actions, generic
Router navigation, and optional diagnostics.

## Forbidden Dependencies

The `core`, `frame`, `host`, and `session` modules must not depend on:

- `native/windows` or `crate::frontend`
- Win32, GDI, Direct2D, or concrete renderer types
- pages, application runtime, business stores, application routes, components, or themes

These boundaries are checked by `tests/architecture.rs`: portable sources reject Windows and
application dependencies, while `platform::win32` separately rejects application dependencies.
The portable dependency boundary is also checked by compiling with no default features. Liuguang's
own architecture tests reject `frontend` dispatcher/host code, lgui facades, and infrastructure
imports routed back through `frontend`.
