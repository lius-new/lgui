# lgui-render-win32-raster

`lgui-render-win32-raster` contains shared Win32 image decoding, CPU blur, static-layer raster,
and cache integration used by the LGUI GDI and Direct2D renderers. Applications should select
`renderer-gdi` or `renderer-d2d` on the `lgui` facade instead of depending on this support crate.
