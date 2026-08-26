# LGUI

`lgui` is the application-independent GUI runtime being extracted from the Liuguang
Windows client. The `0.1.x` migration series is not API-stable and is not published yet.

The current crate contains the backend-independent component runtime, retained host tree,
incremental layout and damage tracking, typed state/effect/context/router primitives, optional
typed Store integration, generic Router history and subscriptions, optional backend-neutral frame
diagnostics, and `UiSession`. Window creation and render backends are still provided by the
application and will move in later phases. The first public controlled widgets are `switch`,
`slider`, and `select`, styled through semantic theme tokens. Diagnostics HUDs and operating-system
sampling remain application adapters.

- [`ARCHITECTURE.md`](ARCHITECTURE.md) defines dependency boundaries.
- [`API.md`](API.md) records the current surface and intended ergonomic direction.
- [`BASELINE.md`](BASELINE.md) records the extraction verification baseline.

Build the portable runtime without optional dependencies:

```powershell
cargo test -p lgui --no-default-features
```

Liuguang currently enables the `tokio` feature, which includes executor-neutral async effects and
the optional Tokio executor adapter.
