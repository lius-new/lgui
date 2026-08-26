# Extraction Baseline

Recorded on 2026-08-26 for the first workspace and runtime extraction.

## Behavioral Contract

- State updates enqueue work and dirty only the owning component.
- Updates from stale component generations cannot recreate unmounted state.
- Effects run only after a committed presentation and clean up on dependency change or unmount.
- Keyed children retain identity across reordering.
- Clean component boundaries retain their Host subtree without visiting descendants.
- Layout updates the nearest affected boundary and reuses paint-only layout results.
- Host commits retain scene nodes and calculate damage from old and new bounds.
- GDI and Direct2D remain application backends consuming the same logical scene and scale data.
- Platform-independent DPI projection rounds outward and preserves visible hairlines.

## Automated Baseline

- `lgui --no-default-features`: 59 unit tests and 1 architecture test pass.
- `lgui --features tokio`: the same 60 tests pass.
- `liugc --bin liugc`: 219 tests pass and 4 environment-dependent live tests remain ignored.
- The portable dependency tree contains only `lgui` itself.
- The `tokio` feature adds only Tokio and its macro dependencies.

Hardware FPS and frame-time numbers are intentionally not treated as portable thresholds. The
Liuguang diagnostics runtime remains the measurement source until diagnostics is extracted; later
performance comparisons must use the same page, viewport, renderer, DPI, build profile, and input
sequence.
