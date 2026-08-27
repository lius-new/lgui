# LGUI Architecture

## Ownership

`Application` owns one `ApplicationContext`, its typed resources, Store registry, Router registry,
WindowManager, platform Dispatcher, and every window session. Each `UiSession` owns component
identity, local State, Effects, the retained host tree, layout state, input state, and scene
commits. Platform backends own native windows, native messages, renderer devices, caches, and
frame presentation.

State handles include component generation identity. Updates dirty only their owning component,
coalesce in the pending update queue, and wake the current Application. Effects run after a
successful present and clean up on dependency changes and unmount.

Stores are Application-scoped pure data. A Store type creates itself lazily on first use. Selector
equality controls component invalidation; mutations do not return UI invalidation values. Router
history and declarative route trees are also Application-scoped. History stores complete Locations;
route matching produces an ancestor-to-leaf chain with decoded parameters and opaque application
metadata. Every nested Outlet is its own retained component boundary, preserving parent component
identity and lifecycle while invalidating only the branch selected by navigation.

`WindowManager` handles typed IDs, explicit owner relationships, show/hide/toggle/close, owner
movement and visibility restoration, DPI changes, input, and native resource release. Auxiliary
windows use the same Application Context, Store, Router, and Dispatcher as the main window.

## Dependency Direction

```text
application root -> lgui public API -> portable runtime -> platform backend
```

The portable modules do not depend on Win32. `platform::win32` does not depend on application
pages, routes, stores, themes, network code, or project environment variables. Business crates
may provide data and resource providers, but they do not wrap or re-export GUI infrastructure.

The GDI and Direct2D factories implement the same Win32 Application renderer contract. With
`advanced-rendering`, both consume the complete Scene model including paths, images, SVG icons,
custom paint, overlays, backdrop blur, static layers, and scroll rasters. Direct2D owns its
D3D11/DXGI/DirectComposition resources and recreates them after resize or presentation failure.

## Features

`--no-default-features` is portable and does not compile Windows, Tokio, images, SVG, or
diagnostics dependencies. Each optional desktop or renderer capability is feature-gated. The
architecture tests recursively enforce the portable/platform and library/application boundaries.
