# lgui-render-skia

Skia rendering backend for [LGUI](https://github.com/lius-new/lgui).

This crate owns scene painting, retained renderer caches, software surfaces,
text layout, and the shared support used by GPU surfaces.

Most applications should depend on the `lgui` facade crate.

## Software text rendering

Applications using a native software window whose presenter discards alpha can
opt into opaque targets before creating their windows:

```rust,ignore
lgui::render_skia::set_opaque_software_windows(true);
```

On Windows, opaque unscaled targets follow the system's font-smoothing and RGB/BGR
pixel-order settings. Unknown pixel geometry, transparent text/layers and
transformed targets use grayscale antialiasing. Font hinting remains `Slight`,
and subpixel positioning remains enabled; enabling LCD does not change metrics.
Standalone `SkiaSoftwareSurface` instances keep transparent behavior by default.

Applications can supply their active UI background when their theme changes:

```rust,ignore
lgui::render_skia::set_software_text_background(background);
```

Windows LCD text on dark software windows then uses a 1.4 coverage gamma to
reduce bright fringes inside small glyphs. Light themes, grayscale targets and
transparent layers keep the original coverage curve. This changes glyph-edge
blending, preserving font weight, size, foreground colors and advances. A coverage
or pixel-order change refreshes all retained software pixels; shaped paragraphs
remain reusable. The two configuration functions are process-wide, so applications
should supply a shared theme before drawing their windows.

Canvas properties use the output-parameter `Canvas::props()` binding. The
Skia 0.99 by-value `Canvas::top_props()` binding faults on Windows/MSVC; the
opaque first-frame regression test covers the native software drawing path.

Run the headless renderer regressions with:

```text
cargo test -p lgui-render-skia --lib
```
