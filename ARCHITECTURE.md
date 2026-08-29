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

`CompositingLayer` is the backend-neutral retained composition boundary. Its descendants compile
into layer-local coordinates and remain in the normal layout, input, accessibility, and popup
trees. The Host keeps the layer as one scene root while still calculating window damage from the
changed descendants. Renderers compare stable command identities to calculate layer-local damage;
movement, insertion, removal, reordering, nested layers, clips, and DPI projection therefore do
not require an unrelated sibling layer to repaint. Popup descendants escape a regular layer and
remain at the top of scene order.

GDI stores each live layer in renderer-scoped DIB surfaces and releases them with the renderer.
Transparent GDI layers use black/white coverage reconstruction so black content, partial alpha,
text, and antialiased edges preserve premultiplied alpha. Direct2D stores each layer in an
`ID2D1Bitmap1`; both backends clear and redraw only layer-local damage and composite only the
intersection with window damage. Size or background-mode changes recreate a surface, opacity-only
changes reuse its pixels, and removed layers are pruned before drawing.

## Features

`--no-default-features` is portable and does not compile Windows, Tokio, images, SVG, or
diagnostics dependencies. Each optional desktop or renderer capability is feature-gated. The
architecture tests recursively enforce the portable/platform and library/application boundaries.
