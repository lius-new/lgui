# LGUI

`lgui` is the application-independent GUI runtime being extracted from the Liuguang
Windows client. The `0.1.x` migration series is not API-stable and is not published yet.

The current crate contains the backend-independent component runtime, retained host tree,
incremental layout and damage tracking, typed state/effect/context/router primitives, optional
typed Store integration, generic Router history and subscriptions, optional backend-neutral frame
diagnostics, and `UiSession`. The default `renderer-gdi` feature provides a runnable Win32
`Application::new()` backend and a basic GDI scene renderer. The optional `backend-win32` layer
also provides clipboard and DPI services plus the layered auxiliary-window host and GDI
backbuffer. Liuguang's resource-aware GDI/Direct2D drawing remains an application adapter for
images, SVG, blur, and custom paint. Public widgets include the layout and display primitives
`button`, `panel`, `stack`, and `text`, plus the controlled `checkbox`, `switch`, `slider`, and
`select` controls styled through semantic theme tokens. Diagnostics HUDs and operating-system
sampling remain application adapters.

- [`ARCHITECTURE.md`](ARCHITECTURE.md) defines dependency boundaries.
- [`API.md`](API.md) records the current surface and intended ergonomic direction.
- [`BASELINE.md`](BASELINE.md) records the extraction verification baseline.

Build the portable runtime without optional dependencies:

```powershell
cargo test -p lgui --no-default-features
```

Run the standalone counter on Windows:

```powershell
cargo run -p lgui --example counter
```

The independent workspace showcase depends only on `lgui` and exercises State, committed Effects,
typed Store selectors/actions, and generic Router navigation:

```powershell
cargo run -p lgui-showcase
cargo run -p lgui-showcase --features diagnostics
```

Liuguang currently enables the `tokio` feature, which includes executor-neutral async effects and
the optional Tokio executor adapter. Disabling default features keeps the crate platform-neutral
and does not pull in Windows, Tokio, diagnostics, or serialization dependencies.
