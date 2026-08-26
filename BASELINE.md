# Extraction Baseline

Recorded on 2026-08-27 after the complete workspace, runtime, platform, and application-boundary
migration.

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

- `lgui --no-default-features`: 72 unit tests and 2 architecture tests pass.
- `lgui --all-features`: 100 unit tests and 2 architecture tests pass.
- `lgui-showcase --all-features` compiles with `lgui` as its only direct dependency.
- The scoped Liuguang binary check and frontend test target pass; backend-wide tests are excluded
  from this migration validation.
- The portable dependency tree contains only `lgui` itself.
- Optional Windows, Tokio, image/SVG, and diagnostics surfaces are absent when default features are
  disabled.

Hardware FPS and frame-time numbers are intentionally not treated as portable thresholds. The
Liuguang diagnostics runtime remains the measurement source until diagnostics is extracted; later
performance comparisons must use the same page, viewport, renderer, DPI, build profile, and input
sequence.
