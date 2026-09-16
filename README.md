# LGUI

[![crates.io](https://img.shields.io/crates/v/lgui.svg)](https://crates.io/crates/lgui)
[![docs.rs](https://docs.rs/lgui/badge.svg)](https://docs.rs/lgui)

`lgui` is an application-independent Rust GUI library. An application supplies one root
component and starts it with `Application::new().run(app)`; `lgui` owns the component session,
retained host tree, event dispatch, reactive updates, windows, renderer, and frame submission.

The portable core provides typed component State, committed Effects, Commands, Events, Context,
Store selectors and actions, declarative Router outlets, layout, input, and scene construction. Optional features add
the Win32 application backend, GDI or Direct2D rendering, multiple windows, images, SVG, advanced
rendering, clipboard, portable notification/tray contracts, Windows notification/tray adapters,
diagnostics, and Tokio execution.

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

Application::new()
    .window_options(WindowOptions::new("counter").size(Size::new(380.0, 240.0)))
    .run(app)?;
```

See [`ARCHITECTURE.md`](ARCHITECTURE.md), [`API.md`](API.md),
[`BASELINE.md`](BASELINE.md), and the implementation/status contract in
[`SKIA_DESIGN.md`](SKIA_DESIGN.md). Build the portable core with:

```powershell
cargo test -p lgui --no-default-features
```

Add the Windows-native default configuration to an application with:

```toml
[dependencies]
lgui = "0.1.0"
```

For the portable winit + Skia backend, disable the Windows-oriented defaults and select a Skia
presentation feature explicitly:

```toml
[dependencies]
lgui = { version = "0.1.0", default-features = false, features = ["renderer-skia-gl", "widgets"] }
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
