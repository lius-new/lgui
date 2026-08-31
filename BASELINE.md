# LGUI Acceptance Baseline

The runtime contract requires component-local State invalidation, stale-handle rejection,
post-present Effects and cleanup, lazy Application-scoped Stores, selector equality, batched Store
updates, declarative Router history/outlets, shared multi-window Context, owner restoration, and a
single Application API for GDI and Direct2D.

The acceptance suite is:

```powershell
cargo check -p lgui --no-default-features
cargo check -p lgui --no-default-features --features backend-win32
cargo check -p lgui --no-default-features --features renderer-gdi
cargo check -p lgui --no-default-features --features renderer-d2d
cargo check -p lgui --no-default-features --features backend-winit
cargo check -p lgui --no-default-features --features notifications,tray
cargo check -p lgui --no-default-features --features renderer-gdi,notifications-win32,tray-win32
cargo check -p lgui --no-default-features --features backend-winit,notifications-win32,tray-win32
cargo test -p lgui --no-default-features --quiet
cargo test -p lgui --no-default-features --features images,persistent-cache --quiet
cargo test -p lgui --no-default-features --features advanced-rendering --quiet
cargo test -p lgui --no-default-features --features renderer-skia --quiet
cargo test -p lgui --no-default-features --features backend-winit --quiet
cargo test -p lgui --quiet
cargo test -p lgui --all-features --quiet
cargo check -p liugc --bin liugc
cargo test -p liugc --bin liugc frontend:: --quiet
cargo test -p liugc --bin liugc backend::settings::storage::tests:: --quiet
cargo check -p lgui-showcase --all-features
cargo fmt -p lgui -- --check
git diff --check
```

Backend-wide tests are outside this GUI boundary and are not part of this baseline. Hardware FPS
is not a portable threshold; performance comparisons must use the same page, viewport, renderer,
DPI, build profile, and input sequence.

Memory acceptance uses the same fixed inputs across GDI, D2D, and Skia. Exercise cold login,
avatar loading, the Store list, route round trips, dialogs, theme/scale changes, simultaneous
windows, hide/restore, and device recovery. Capture `MemorySnapshot`, working set, private bytes,
and available GPU memory after warm-up, at the operation peak, after leaving the page, after all
windows are hidden, and after restore. Repeated churn must settle below the selected profile's
soft budgets after owner-thread Trim work drains; visible pinned bytes may exceed a cache share but
must be reported as pinned overflow rather than silently discarded.

Release acceptance also verifies that the Windows settings page restores version-6 memory
settings, profile changes rebalance existing domains, memory and persistent clear actions report
results, `memory.snapshot`/`memory.trim` work with diagnostics enabled, and no native resource is
dropped outside its owning UI thread.
