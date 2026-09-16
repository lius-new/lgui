# Workspace crates

Every directory in this folder is a publishable Cargo package and has the same name as its
`package.name`. The root `lgui` package is the application-facing facade.

| Package | Owns |
| --- | --- |
| `lgui-core` | Portable application runtime, retained UI tree, layout, input, Scene, windows, and memory governance |
| `lgui-assets` | Image loading and caching, render resources, custom paint, and SVG icons |
| `lgui-diagnostics` | Frame metrics, collectors, and diagnostics providers |
| `lgui-services` | Desktop service contracts and optional system adapters |
| `lgui-router` | Route history, matching, declarative route trees, outlets, and navigation hooks |
| `lgui-store` | Application-scoped stores, selectors, actions, and subscriptions |
| `lgui-widgets` | Theme tokens and reusable controls |
| `lgui-render-api` | Backend-neutral frame and renderer contracts |
| `lgui-render-skia` | Portable Skia scene renderer |
| `lgui-render-gdi` | Native GDI renderer and optional retained GDI pipeline |
| `lgui-render-d2d` | Direct2D, D3D11, DXGI, and DirectComposition renderer |
| `lgui-render-win32-raster` | Shared Win32 image, blur, static-layer, and raster-cache support |
| `lgui-platform-winit` | Winit event loop, windows, input, and Skia surface adapters |
| `lgui-platform-win32` | Win32 windows, message dispatch, native services, and renderer host contract |

Dependency direction is kept acyclic: add-on crates depend on `lgui-core`; renderers depend on
`lgui-core` and `lgui-render-api`; platform packages host renderer contracts and surfaces; the root
facade composes the selected packages through features.
