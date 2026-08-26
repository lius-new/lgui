# UI Core Runtime

`core` is the backend-independent retained UI runtime. The extraction boundary and public-crate
constraints live in [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md). The original runtime design
and acceptance criteria remain in the Liuguang application migration documentation.

## Public component model

Application UI is expressed as retained function components and declarative `Element` content:

```rust
fn counter(cx: &mut RenderCx<'_, '_>) -> Element {
    let (count, set_count) = cx.use_state(|| 0_u32);

    cx.use_effect((count,), move || {
        move || tracing::debug!(count, "counter effect cleanup")
    });

    group(UiRect::new(0, 0, 160, 48)).content((
        count,
        button("Add").on_click(move || set_count(count + 1)),
    ))
}
```

`Element::on_click` accepts a zero-argument closure. Code that needs propagation control,
default prevention, async spawning, or a window request uses `Element::on_click_event`.

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
Platform code translates native messages to `InputEvent`; application layers provide theme,
Store adapters, and route types. Control-specific retained state (text editing, selection, scroll,
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
cargo test -p lgui --no-default-features
cargo test -p lgui --features tokio
cargo check -p liugc --all-features --all-targets
cargo test -p liugc --bin liugc
```

Then run `git diff --check` from the repository root. Architecture scans must find no compatibility
markers, deprecated allowances, old `cx.component` calls, business `ui_dispatcher` calls, or Core
dependencies on platform/application modules.
