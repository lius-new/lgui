# LGUI Architecture

## Ownership

`Application` owns one `ApplicationContext`, one `MemoryGovernor`, typed resources, Store and
Router registries, the Command registry, Event bus, platform dispatcher, and all window sessions. The top-level
`window` subsystem owns the platform-neutral `WindowManager`, handles, IDs, options, and command
model. Application re-exports the established window types only as a compatibility facade.
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

`services` owns platform-neutral clipboard, notification, and tray contracts. Application owns
only service registration and application-action orchestration. Windows Toast, notification-icon,
and tray message-loop code belongs to `platform::win32::services`; those adapters do not own an
`ApplicationContext`, business command handler, or main-window handle.

## Dependency Direction

```text
application root -> lgui public API -> portable runtime/contracts -> platform adapters
```

Portable modules do not depend on Win32 or Winit. `services` does not depend on `application` or
`platform`; platform modules implement its contracts. `platform::win32` does not depend on pages,
business stores, network protocols, project environment variables, or Liuguang-specific paint
keys. Business crates may provide data and resources, but they do not wrap or re-export GUI
infrastructure.

## Memory Governance

Memory governance is Application-scoped. `ApplicationContext` owns the only `MemoryGovernor`;
asset runtimes, sessions, renderers, and application-owned diagnostic buffers register adapters
under a `CacheDomain`. A registration reports `CacheUsage`, accepts an assigned share of the
Application CPU or native soft budget, and handles bounded `TrimRequest`s. Dropping the
registration removes that domain instance from snapshots.

The Governor does not own cache entries or native handles. Portable caches may trim inline.
Win32 adapters post work through `Win32Dispatcher`, and Winit adapters send a user event, so GDI,
D2D, Skia, Component, Host, Scene, and other native or retained objects are released on their
owning UI thread. A synchronous Trim result may therefore report only immediately confirmed
bytes; a later snapshot observes releases completed by the owner thread.

Usage separates live, rebuildable, cache, CPU, estimated GPU, pinned, transient-reserved, and
persistent bytes. Low-memory, balanced, and performance profiles provide one shared CPU budget,
one shared native budget, a transient hard limit, and a persistent quota. Domain weights divide
the shared totals between current registrations; individual caches must not add independent
uncoordinated totals. Soft-budget enforcement runs after frame commits and option changes. Window
hide, all-windows-hidden, Session unmount, device loss, theme or scale changes, explicit pressure,
and Application shutdown enter the same event and Trim path.

Scene image reachability is aggregated by window instance. `ImageRequest` carries retention,
decode, priority, namespace, version, and sensitivity metadata through Scene primitives to each
backend. Encoded and decoded image caches coalesce in-flight work, apply failure backoff, validate
encoded and decoded bounds, and reserve large-task bytes before work begins. Static and scroll
raster caches use explicit policies and byte bounds; the former pseudo-disk raster map and all
cache-specific public clear paths are removed.

With `persistent-cache`, an application may inject a `PersistentCacheStore`. The file store keeps
only portable compressed bytes and metadata, uses atomic replacement and SHA-256 validation, and
enforces TTL, validators, namespace/version keys, and an on-disk LRU quota. Sensitive requests are
not persisted. `lgui` never chooses an application directory or reads business settings.

The Liuguang Windows client maps its version-6 settings to `MemoryOptions`, injects a cache
subdirectory, exposes profile/quota/clear controls, and adds bounded memory history to diagnostics.
It remains a configuration and presentation consumer; resource loading and eviction stay in
`lgui`.

## Source Layout

The physical tree follows subsystem ownership:

```text
src/
|-- application/   # builders, context, handles, lifecycle and service registration
|-- command/       # typed contracts, context, registry, handles
|-- events/        # typed contracts, bus, subscriptions
|-- router/        # hooks, matching, runtime history, declarative routes
|-- store/         # definitions, hooks, subscriptions, runtime registry
|-- core/          # foundation, view, component, input, layout, scene
|-- runtime/       # session, retained host, frame invalidation
|-- window/        # IDs, options, handles, manager and portable commands
|-- renderer/      # portable contract/cache and portable Skia renderer
|-- platform/      # Winit adapters and categorized Win32 implementation
|-- services/      # portable clipboard, notification, tray, dialog and URL contracts
|-- assets/        # image/resource system and icons
|-- memory/        # Application budgets, domains, reservations, Trim, stats, persistence
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
services/clipboard/                     # contract and optional system adapter
services/notification/                  # model, service contract, type-erased handle
services/tray/                          # generic menu contract and application actions
platform/win32/services/tray/           # icon, host loop, native menu and shared support
window/                                 # portable window model and command transport
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
- `notifications` and `tray` enable only portable APIs. `notifications-win32` and `tray-win32`
  add the Windows adapters through `windows-platform`, without forcing the Win32 window backend.
- Accessibility, images, SVG, and diagnostics stay independently gated.
- `persistent-cache` adds the portable store contract and file-store implementation without
  enabling a renderer or platform backend.

The CI matrix checks portable no-default tests, each Windows backend boundary, standalone Winit,
default tests, and all-feature tests. Architecture tests enforce directory ownership, dependency
direction, real-module assembly, and the feature graph.
