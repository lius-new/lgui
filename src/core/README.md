# UI Core Runtime

`core` is the backend-independent retained UI runtime. The library boundary and public-crate
constraints live in [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md).

## Public component model

Application UI is expressed as retained function components and declarative `Element` content:

```rust
fn counter(cx: &mut RenderCx<'_, '_>) -> Element {
    let count = cx.state(0_u32);
    let current = count.get();
    let rect = UiRect::new(0, 0, 160, 48);

    cx.use_effect((current,), move || {
        move || tracing::debug!(count = current, "counter effect cleanup")
    });

    group(rect).content((
        current,
        button(rect, "Add", ButtonStyle::default())
            .on_click(move |_| count.update(|value| *value += 1)),
    ))
}
```

Element handlers receive the generic `UiEventContext`, which provides propagation control,
default prevention, async spawning, Store/Router access, and the current Window handle.

## Ownership and update flow

- `ComponentTree` owns component identity, keyed reconciliation, cached output, Hook order, and
  component dirtiness.
- `HookStateStore` and `UiUpdateQueue` own typed State, thread-safe batched updates and
  session-bound focus requests.
- `EffectRegistry` stages Effects during render and runs them only after a successful present;
  dependency changes and unmounts run cleanup first.
- `ContextRegistry`, `Observable`, and `RouterContext` provide typed Context, Store selectors, and
  application-defined routing without business commands in Core.
- `HostTree` is the retained backend-independent projection. A clean component boundary keeps its
  Host subtree without visiting descendants.
- `LayoutRuntime` consumes projection changes and lays out only affected boundaries.
- `HostRuntime` reconciles generated Host mutations, retained Scene nodes, and commit-driven damage.
- GDI and Direct2D encode the same retained `Scene` owned by a window-level `UiSession`.

The normal update chain is:

```text
generic input -> Element handler -> queued update -> dirty component execution
  -> Host projection changes -> incremental layout -> retained Scene update
  -> damage -> backend present -> committed Effects
```

## Dependency boundary

Core contains no Win32, GDI, Direct2D, page, application route, settings, or domain-store types.
Platform code translates native messages to `InputEvent`; applications provide theme data,
pure Store types, and route values. Control-specific retained state (text editing, selection, scroll,
slider, rich editor) remains separate from component Hook State.

## Identity rules

- Components use generated `ComponentId { index, generation }` identities.
- Host runtime nodes use generated `HostNodeId { index, generation }` identities.
- Unkeyed children have position semantics; `.key(...)` gives stable list identity.
- Application pages do not construct `UiScope`, `UiRenderContext`, or render IDs.
- Stale State setters and async results cannot recreate an unmounted component generation.

## Required verification

Run from the `native` workspace:

```powershell
cargo fmt --all --check
cargo test -p lgui --no-default-features --quiet
cargo test -p lgui --all-features --quiet
cargo check -p liugc --bin liugc
cargo test -p liugc --bin liugc frontend:: --quiet
cargo check -p lgui-showcase --all-features
```

Then run `git diff --check` from the repository root. Architecture scans must find no compatibility
markers, deprecated allowances, old `cx.component` calls, business `ui_dispatcher` calls, or Core
dependencies on platform/application modules.
