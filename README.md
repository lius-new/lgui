# LGUI

`lgui` is an application-independent Rust GUI library. An application supplies one root
component and starts it with `Application::new().run(app)`; `lgui` owns the component session,
retained host tree, event dispatch, reactive updates, windows, renderer, and frame submission.

The portable core provides typed component State, committed Effects, Context, Store selectors and
actions, declarative Router outlets, layout, input, and scene construction. Optional features add
the Win32 application backend, GDI or Direct2D rendering, multiple windows, images, SVG, advanced
rendering, clipboard, notifications, tray integration, diagnostics, and Tokio execution.

Application resources are ordinary typed data. Asset resolvers and custom paint providers are
provided to `Application`, while renderers own their native caches and device resources. Business
crates do not create a second runtime, dispatcher, presenter, WndProc, or window registry.

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

The Skia desktop backend uses winit with portable OpenGL or software presentation. Liuguang keeps
Skia behind an explicit feature and runtime renderer choice during migration:

```powershell
cargo run --bin liugc --features renderer-skia -- --renderer skia
cargo run --bin liugc --features renderer-skia -- --renderer skia-opengl
cargo run --bin liugc --features renderer-skia -- --renderer skia-software
```
