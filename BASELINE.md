# LGUI Acceptance Baseline

The runtime contract requires component-local State invalidation, stale-handle rejection,
post-present Effects and cleanup, lazy Application-scoped Stores, selector equality, batched Store
updates, declarative Router history/outlets, shared multi-window Context, owner restoration, and a
single Application API for GDI and Direct2D.

The acceptance suite is:

```powershell
cargo test -p lgui --no-default-features --quiet
cargo test -p lgui --all-features --quiet
cargo check -p liugc --bin liugc
cargo test -p liugc --bin liugc frontend:: --quiet
cargo check -p lgui-showcase --all-features
cargo fmt --all --check
git diff --check
```

Backend-wide tests are outside this GUI boundary and are not part of this baseline. Hardware FPS
is not a portable threshold; performance comparisons must use the same page, viewport, renderer,
DPI, build profile, and input sequence.
