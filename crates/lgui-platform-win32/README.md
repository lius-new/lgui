# lgui-platform-win32

Native Win32 windows, message dispatch, system services, and renderer host contract for
[LGUI](https://github.com/lius-new/lgui). Renderer implementations live in the independent
`lgui-render-gdi` and `lgui-render-d2d` packages; their shared raster support lives in
`lgui-render-win32-raster`.

Most applications should depend on the `lgui` facade crate.
