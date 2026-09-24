# LGUI

[![crates.io](https://img.shields.io/crates/v/lgui.svg)](https://crates.io/crates/lgui)
[![docs.rs](https://docs.rs/lgui/badge.svg)](https://docs.rs/lgui)

`lgui` is an application-independent Rust GUI library. An application supplies one root
component, chooses a platform backend, and starts it with `Application::with_backend(...)`;
`lgui` owns the component session, retained host tree, event dispatch, reactive updates, windows,
renderer, and frame submission.

The portable core provides typed component State, committed Effects, Commands, Events, Context,
layout, input, and scene construction. Separate add-on crates provide Store, Router, themes, and
widgets. Optional features compose those packages with the winit platform backend, Skia
rendering, images, SVG, desktop services, diagnostics, and Tokio execution.

Application resources are ordinary typed data. Asset resolvers and custom paint providers are
provided to `Application`, while renderers own their native caches and device resources. Business
crates do not create a second runtime, dispatcher, presenter, WndProc, or window registry.

Commands and Events are typed, Application-scoped capabilities. Commands use receiver-free
`invoke::<C>(args).await`; Events use a stable `EventKey<T>` with
`emit(key, payload).await`, `listen(key, listener)`, and `listen_async(key, listener)`.
Neither path requires serialization, IPC, or a Store dependency. Applications define the
concrete contracts and register service-backed command handlers.

```rust,ignore
fn app(cx: &mut RenderCx<'_, '_>) -> Element {
    let count = cx.state(0_i32);
    let increment = count.clone();

    stack(UiRect::new(0.0, 0.0, 360.0, 200.0), Axis::Vertical)
        .content((
            text(UiRect::new(0.0, 0.0, 312.0, 56.0), format!("{}", count.get()), TextStyle::default()),
            button(
                UiRect::new(0.0, 0.0, 312.0, 48.0),
                "one up",
                ButtonStyle::default(),
            )
                .on_click(move |_| increment.update(|value| *value += 1)),
        ))
        .into()
}

Application::with_backend(WinitApplication::new(GraphicsPreference::Auto))
    .provide(RendererKind::Skia(GraphicsPreference::Auto))
    .memory_options(MemoryOptions::unbounded(
        ImageCachePolicy::WhileVisible,
        false,
    ))
    .window_options(WindowOptions::new("counter").size(Size::new(380.0, 240.0)))
    .run(app)?;
```

The workspace publishes twelve packages with one owner for each responsibility:

| Package | Responsibility |
| --- | --- |
| `lgui` | Application-facing facade and feature composition |
| `lgui-core` | Portable application runtime, components, layout, input, Scene, windows, and memory governance |
| `lgui-assets` | Image loading and caching, render resources, custom paint providers, and SVG icons |
| `lgui-diagnostics` | Frame samples, renderer metrics, collectors, and diagnostics providers |
| `lgui-services` | Clipboard, dialogs, URL opening, notifications, and tray contracts and adapters |
| `lgui-router` | Route history, matching, declarative routes, outlets, and navigation hooks |
| `lgui-store` | Application-scoped stores, selectors, actions, and subscriptions |
| `lgui-widgets` | Theme tokens and reusable controls |
| `lgui-render-api` | Frame, damage, renderer lifecycle, and memory-pressure contracts |
| `lgui-render-skia` | Skia scene painting, text layout, software surface, and renderer caches |
| `lgui-platform-winit` | Portable desktop windows, input, event loop, and Skia surfaces |
| `lgui-platform-win32` | Native Win32 desktop services (tray, notifications, diagnostics), SVG icon rasterization, and native pixel interop |

Applications should normally depend only on `lgui`; the other packages are public so renderer and
platform integrations can be developed and released independently.

See [`ARCHITECTURE.md`](ARCHITECTURE.md), [`API.md`](API.md),
[`BASELINE.md`](BASELINE.md), and the implementation/status contract in
[`SKIA_DESIGN.md`](SKIA_DESIGN.md). Build the portable core with:

```powershell
cargo test -p lgui-core --no-default-features
```

Add the Windows-native default configuration to an application with:

```toml
[dependencies]
lgui = "0.2.2"
```

For the portable winit + Skia backend, disable the Windows-oriented defaults and select a Skia
presentation feature explicitly:

```toml
[dependencies]
lgui = { version = "0.2.2", default-features = false, features = ["renderer-skia-gl", "widgets"] }
```

Run the same example source through the portable winit + Skia backend on Windows, Linux, or
macOS:

```powershell
cargo run --example counter --no-default-features --features renderer-skia-gl,widgets
```

The Skia desktop backend uses winit with Vulkan, OpenGL, Metal, or software presentation.
Applications on macOS may enable `renderer-skia-metal`; Linux and Windows applications may enable
their platform-specific Vulkan feature. Explicit GPU choices return an error when the requested
driver is unavailable; only `Auto` follows the bounded fallback chain.

## Related projects

- [Loom](https://github.com/lius-new/loom) — a text editor built with LGUI.

## License

LGUI is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT License](LICENSE-MIT), at your option.
