# lgui-render-skia

Skia rendering backend for [LGUI](https://github.com/lius-new/lgui).

This crate owns scene painting, retained renderer caches, software surfaces,
text layout, and the shared support used by GPU surfaces.

Most applications should depend on the `lgui` facade crate.
