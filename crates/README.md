# Workspace crates

Every directory in this folder is a publishable Cargo package and has the same name as its
`package.name`. The root `lgui` package is the application-facing facade.

| Package | Owns |
| --- | --- |
| `lgui-core` | Portable application runtime, retained UI tree, layout, input, Scene, memory, assets, and service contracts |
| `lgui-router` | Route history, matching, declarative route trees, outlets, and navigation hooks |
| `lgui-store` | Application-scoped stores, selectors, actions, and subscriptions |
| `lgui-widgets` | Theme tokens and reusable controls |
| `lgui-render-api` | Backend-neutral frame and renderer contracts |
| `lgui-render-skia` | Portable Skia scene renderer |
| `lgui-render-win32` | GDI and Direct2D renderers plus their shared Windows render caches |
| `lgui-platform-winit` | Winit event loop, windows, input, and Skia surface adapters |
| `lgui-platform-win32` | Win32 windows, message dispatch, native services, and renderer host contract |

Dependency direction is kept acyclic: add-on crates depend on `lgui-core`; renderers depend on
`lgui-core` and `lgui-render-api`; platform packages host renderer contracts and surfaces; the root
facade composes the selected packages through features.
