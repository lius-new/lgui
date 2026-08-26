# LGUI

`lgui` is the application-independent GUI runtime being extracted from the Liuguang
Windows client. The `0.1.x` migration series is not API-stable and is not published yet.

The current crate contains the backend-independent component runtime, retained host tree,
incremental layout and damage tracking, generic state/effect/context/router primitives, and
`UiSession`. Window creation, render backends, widgets, Store integration, and diagnostics are
still provided by the application and will move in later phases.

- [`ARCHITECTURE.md`](ARCHITECTURE.md) defines dependency boundaries.
- [`API.md`](API.md) records the current surface and intended ergonomic direction.
- [`BASELINE.md`](BASELINE.md) records the extraction verification baseline.

Build the portable runtime without optional dependencies:

```powershell
cargo test -p lgui --no-default-features
```

Liuguang currently enables the `tokio` feature for cancellable async effects.
