# lgui-render-win32

GDI and Direct2D scene renderers for the LGUI Win32 platform host. The package owns renderer
devices, retained surfaces, image and effect caches, and renderer memory adapters.

Most applications should select `renderer-gdi` or `renderer-d2d` on the `lgui` facade crate.
