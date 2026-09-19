# lgui-platform-win32

Native Win32 desktop services (tray, notifications, system diagnostics), SVG icon rasterization,
and native pixel interop for [LGUI](https://github.com/lius-new/lgui). Rendering is provided by the
portable `lgui-render-skia` package hosted through the winit backend.

Most applications should depend on the `lgui` facade crate.
