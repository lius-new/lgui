# LGUI Architecture

## Ownership

`Application` owns one `ApplicationContext`, typed resources, Store and Router registries,
the Command registry, Event bus, WindowManager, platform dispatcher, and all window sessions.
Each `UiSession` owns component identity, local State, Effects, the retained host tree, layout,
input state, and scene commits. Platform backends own native windows, renderer devices, resource
caches, and frame presentation.

State handles carry component-generation identity. Updates invalidate only their owning component,
coalesce in the pending queue, and wake the current Application. Effects run after a successful
present and clean up when dependencies change or the component unmounts.

Stores are Application-scoped data. Selector equality controls component invalidation. Router
history and declarative route trees are also Application-scoped; nested Outlets retain independent
component boundaries. Commands are typed request/response contracts, and Events are typed
broadcasts. Neither subsystem depends on Store, Router, a platform backend, or application
business types.

`WindowManager` owns typed IDs, owner relationships, visibility, close policy, DPI changes,
input, and native resource release. Auxiliary windows share the same Application Context, Store,
Router, and Dispatcher as the main window.

## Dependency Direction

```text
application root -> lgui public API -> portable runtime -> platform backend
```

Portable modules do not depend on Win32 or Winit. `platform::win32` does not depend on pages,
business stores, network protocols, project environment variables, or Liuguang-specific paint
keys. Business crates may provide data and resources, but they do not wrap or re-export GUI
infrastructure.

## Source Layout

The physical tree follows subsystem ownership:

```text
src/
|-- application/   # builders, context, handles, window registration
|-- command/       # typed contracts, context, registry, handles
|-- events/        # typed contracts, bus, subscriptions
|-- router/        # hooks, matching, runtime history, declarative routes
|-- store/         # definitions, hooks, subscriptions, runtime registry
|-- core/          # foundation, view, component, input, layout, scene
|-- runtime/       # session, retained host, frame invalidation
|-- renderer/      # portable contract/cache and portable Skia renderer
|-- platform/      # Winit adapters and categorized Win32 implementation
|-- services/      # clipboard, dialogs, URL opening, service contracts
|-- assets/        # image/resource system and icons
|-- text/          # text model, layout, and text-system boundary
|-- theme/         # theme tokens and context
|-- widgets/       # controls; complex controls own subdirectories
`-- diagnostics/   # model, collection, providers, timing
```

`runtime` physically owns Session, Host, and Frame. The established `lgui::session`,
`lgui::host`, and `lgui::frame` paths remain compatibility entry points. Core keeps the
established flat `lgui::core::*` exports while its implementation is grouped by responsibility.

Large implementations are split one level further:

```text
core/component/runtime/                 # state, input, action, animation, focus
core/view/declarative/                  # element, content, events, primitives
core/view/tree/                         # mutation, event dispatch, scene projection
core/scene/render/                      # compiler, transform, phase, scene, damage
runtime/host/                           # model, storage, reconcile, commit, scene
renderer/skia/backend/                  # support, text, cache, software, painter
platform/win32/application/host/        # contract, state, backend, window, loop
platform/win32/renderer/enhanced/d2d/   # cache, drawing, effects, resources
platform/win32/renderer/enhanced/gdi_renderer/
platform/win32/renderer/enhanced/static_layer/
```

These directories are real Rust submodules. Source assembly with `include!` is forbidden:
cross-file dependencies must be visible through an explicit module boundary and the narrowest
practical visibility, normally `pub(super)`.

Large test suites live in sibling `tests.rs` files or `tests/` directories. Production
facades contain module declarations, shared contracts, and re-exports rather than hundreds of
lines of test code.

## Rendering

The GDI and Direct2D factories implement the same Win32 Application renderer contract. Both
consume the complete Scene model. Direct2D owns its D3D11, DXGI, and DirectComposition resources
and recreates them after resize or presentation failure.

`CompositingLayer` is the backend-neutral retained composition boundary. Its children use
layer-local coordinates while staying in the normal layout, input, accessibility, and popup
trees. Renderers compare stable command identities to calculate local damage. Size and background
changes recreate a surface; opacity and transform changes reuse its pixels.

Scene primitive translation is implemented once in `core::scene::render::transform`. Portable
scene compilation translates nested static-layer commands, while backend rasterization preserves
the already-local command list through an explicit policy. D2D, GDI static layers, and other
backend consumers do not maintain private copies of the primitive transform match.

GDI retains live compositing layers in renderer-scoped DIB surfaces. Transparent layers use
black/white coverage reconstruction to preserve premultiplied alpha. Direct2D retains layers in
`ID2D1Bitmap1` surfaces. Both redraw only layer-local damage and composite only its intersection
with window damage.

## Features

`--no-default-features` is portable and compiles no window backend, Tokio runtime, image stack,
SVG stack, or diagnostics provider.

Feature ownership is explicit:

- `backend-win32` enables only the Win32 platform boundary.
- `renderer-gdi` and `renderer-d2d` require `backend-win32`.
- `backend-winit` requires `renderer-skia`, because its software fallback and presentation
  path are Skia-based.
- `renderer-skia` remains portable and does not enable Win32.
- GL, Vulkan, and Metal features add only their Winit surface adapters and target dependencies.
- Accessibility, notifications, tray, images, SVG, and diagnostics stay independently gated.

The CI matrix checks portable no-default tests, each Windows backend boundary, standalone Winit,
default tests, and all-feature tests. Architecture tests enforce directory ownership, dependency
direction, real-module assembly, and the feature graph.
