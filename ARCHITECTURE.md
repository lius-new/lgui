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

The first extraction preserves the existing `RenderCx`, `Element`, state, effect, context,
observable, generic router, retained host tree, layout, damage, and `UiSession` semantics. New API
design is deferred until the application compiles against this physical crate boundary.

Application-specific behavior is attached to `UiEventContext` through traits defined in the
application crate. The GUI crate exposes only propagation, scheduling flags, application context
access, and an opaque host-command slot. A small set of `#[doc(hidden)]` component-builder methods
remains public while widgets still live in the application crate; they return to crate-private
visibility when the widgets migration is complete.

Async effects are available with the `tokio` feature during the migration. The core task-spawner
contract remains executor-neutral; a later phase will remove Tokio-specific cancellation from the
component runtime.

## Forbidden Dependencies

The `core`, `frame`, `host`, and `session` modules must not depend on:

- `native/windows` or `crate::frontend`
- Win32, GDI, Direct2D, or concrete renderer types
- pages, application runtime, business stores, application routes, components, or themes

These boundaries are checked by `tests/architecture.rs` and by compiling the crate with no default
features.
