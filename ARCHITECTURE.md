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
component boundaries. Commands are typed request/response contracts. Events are keyed broadcasts:
the stable string in `EventKey<T>` is the routing identity and `T` is the payload contract. Neither
subsystem depends on Store, Router, a platform backend, or application business types.

Receiver-free `invoke` and `emit` resolve the active Application from a poll-scoped context. LGUI
task entry points restore that context on every Future poll, so executor thread migration cannot
cross Application boundaries. Futures submitted to an external executor must be wrapped once by
their owning `ApplicationContext::scope`; no process-global Application singleton is used.

`WindowManager` owns typed IDs, owner relationships, visibility, close policy, DPI changes,
input, and native resource release. Auxiliary windows share the same Application Context, Store,
Router, and Dispatcher as the main window.

`lgui-services` owns platform-neutral clipboard, notification, dialog, URL-opening, and tray
contracts plus their Application extension traits. Windows Toast, notification-icon,
and tray message-loop code belongs to `platform::win32::services`; those adapters do not own an
`ApplicationContext`, business command handler, or main-window handle.

`lgui-assets` owns image resolution and caching, render resources, custom paint providers, and
SVG icon registration. `lgui-diagnostics` owns frame and machine metrics, collection, and sink
registration. Both depend on `lgui-core` contracts and remain independent of platform backends.

## Dependency Direction

```text
application root -> lgui public API -> portable runtime/contracts -> platform adapters
```

Portable capability crates depend inward on `lgui-core`, never on Win32 or Winit. Platform crates
consume their contracts and implement native adapters. `platform::win32` does not depend on pages,
business stores, network protocols, project environment variables, or Liuguang-specific paint
keys. Business crates may provide data and resources, but they do not wrap or re-export GUI
infrastructure.

## Memory Governance

Memory governance is Application-scoped. `ApplicationContext` owns the only `MemoryGovernor`;
asset runtimes, sessions, renderers, and application-owned diagnostic buffers register adapters
under a `CacheDomain`. A registration reports `CacheUsage`, accepts an assigned share of the
application-provided budget for its exact domain, and handles bounded `TrimRequest`s. Multiple
instances divide only their common domain total; dropping a registration removes it from
snapshots and rebalances the survivors. Zero is a valid domain budget and is forwarded unchanged.

The Governor does not own cache entries or native handles. Portable caches may trim inline.
Win32 adapters always post coalesced work through `Win32Dispatcher`; Winit adapters send a user
event. Skia, Component, Host, Scene, and other native or retained objects are therefore
released on their owning UI thread without extending the caller's input or frame stack. A later
snapshot observes asynchronously completed native work.

Usage separates live, rebuildable, cache, CPU, estimated GPU, pinned, transient-reserved, and
persistent bytes. `lgui` defines no profile, budget values, domain weights, lifecycle defaults, or
implicit fallback policy. Application construction requires an explicit, validated
`MemoryOptions`; it contains global limits, exact domain totals, lifecycle event actions, the
default image policy, and the persistent-cache choice. `MemoryOptions::unbounded` is available
only as an explicit application decision and still requires an explicit persistent-cache choice.
Framework caches begin at zero until registration assigns the application's domain budget;
backends do not clamp that value or raise zero to a framework minimum. Window, Session, device,
pressure, and shutdown events enter one path, then execute the action selected by the
application's `MemoryEventPolicy`.

Frame budget enforcement reads lightweight domain usage at a bounded interval and trims only when
evictable bytes cross the hard limit, targeting the soft limit for hysteresis. Pinned resources and
HostScene state are reported but excluded from ordinary cache enforcement. Win32 owner-thread Trim
requests are coalesced per adapter, retaining the strictest pending target so repeated notifications
cannot flood the UI dispatcher.

Scene image reachability is aggregated by window instance. `ImageRequest` carries retention,
decode, priority, namespace, version, and sensitivity metadata through Scene primitives to each
backend. Encoded and decoded image caches coalesce in-flight work, apply failure backoff, validate
encoded and decoded bounds, and reserve large-task bytes before work begins. Static and scroll
raster caches use explicit policies and byte bounds; the former pseudo-disk raster map and all
cache-specific public clear paths are removed. Reachable image bytes are pinned long enough for
delivery even at a zero reusable-cache budget, then become eviction candidates when unreachable.
The built-in remote-image transport uses the operating system certificate store and exposes no
private-CA setting or certificate-validation bypass. Win32 image completion notifications
coalesce cache keys and damage only the bounds of Scene image nodes that reference those keys.

With `persistent-cache`, an application may inject a `PersistentCacheStore`. The file store keeps
only portable compressed bytes and metadata, uses atomic replacement and SHA-256 validation, and
enforces TTL, validators, namespace/version keys, and an on-disk LRU quota. Sensitive requests are
not persisted. `CacheScope::Persistent` is routed only to this store, while `Memory` and
`AllRebuildable` remain in-memory scopes. `lgui` never chooses an application directory or reads
business settings.

The Liuguang Windows client owns its profile names and numeric limits in
`windows/src/memory_policy.rs`. Its embedded `windows/client.toml` selects one fixed application
profile, persistent-cache enablement, and disk quota; startup maps `APP_CONFIG.memory` to
`MemoryOptions` and injects a cache subdirectory only when enabled. These are deployment choices,
not user Settings controls. Other applications can choose entirely different configuration,
limits, and event actions. Resource loading, accounting, and policy execution remain in `lgui`;
the policy itself remains in the application.

## Source Layout

The physical tree follows subsystem ownership:

```text
src/                           # lgui facade and feature composition
crates/
|-- lgui-core/src/             # portable runtime, UI model, windows, memory
|-- lgui-assets/src/           # image runtime, render resources, custom paint, SVG icons
|-- lgui-diagnostics/src/      # metrics, samples, collectors, providers
|-- lgui-services/src/         # desktop service contracts and optional system adapters
|-- lgui-router/src/router/    # matching, history, declarative routes, outlets
|-- lgui-store/src/store/      # stores, selectors, actions, subscriptions
|-- lgui-widgets/src/          # theme tokens and reusable controls
|-- lgui-render-api/src/       # backend-neutral renderer lifecycle
|-- lgui-render-skia/src/      # portable Skia renderer
|-- lgui-platform-winit/src/   # event loop, input, windows, Skia surfaces
`-- lgui-platform-win32/src/   # system adapters, SVG icons, pixel interop
```

`lgui-core` owns only contracts and runtime capabilities required by every application. Optional
assets, diagnostics, and desktop services depend on core as sibling crates and are composed by the
`lgui` facade. `runtime` physically owns Session, Host, and Frame. The established `lgui::session`,
`lgui::host`, and `lgui::frame` paths remain compatibility entry points. Core keeps the
established flat `lgui::core::*` exports while its implementation is grouped by responsibility.

The `core` implementation is divided into real Rust modules: `foundation` owns geometry, IDs, and
styles; `component` owns component identity, hooks, effects, and update execution; `input` owns the
portable event model and dispatch; `layout` owns layout and dirtiness; `scene` owns backend-neutral
primitives and compilation; and `view` owns declarative elements and the retained host projection.
Concrete backend selection and probing belong to the `lgui` facade, while renderer preference is a
portable contract in `lgui-render-api`. `lgui-core` exposes no Skia, Win32, or Winit
feature.

Large implementations are split one level further:

```text
lgui-core/src/core/component/runtime/                    # state, input, action, animation, focus
lgui-core/src/core/component/component_tree/             # identity, lifecycle, output memory, storage
lgui-core/src/core/input/event/                          # dispatch and animation event responses
lgui-core/src/core/view/declarative/                     # element, content, events, primitives
lgui-core/src/core/scene/render/                         # compiler, projection, signatures, cache, damage
lgui-core/src/runtime/host/                              # retained model and commit pipeline
lgui-router/src/router/declarative/                      # routes and retained outlets
lgui-render-skia/src/backend/                            # text, cache, software, painter
lgui-platform-win32/src/services/tray/                   # native tray adapter
```

These directories are real Rust submodules. Source assembly with `include!` is forbidden:
cross-file dependencies must be visible through an explicit module boundary and the narrowest
practical visibility, normally `pub(super)`.

Large test suites live in sibling `tests.rs` files or `tests/` directories. Production
facades contain module declarations, shared contracts, and re-exports rather than hundreds of
lines of test code.

## Rendering

Skia is the single renderer backend,
hosted by the winit surface adapters and owning its own text, surface, cache, and bitmap
resources.

`CompositingLayer` is the backend-neutral retained composition boundary. Its children use
layer-local coordinates while staying in the normal layout, input, accessibility, and popup
trees. Renderers compare stable command identities to calculate local damage. Size and background
changes recreate a surface; opacity and transform changes reuse its pixels.

Scene primitive translation is implemented once in `core::scene::render::transform`. Portable
scene compilation translates nested static-layer commands, while backend rasterization preserves
the already-local command list through an explicit policy. Backend
consumers do not maintain private copies of the primitive transform match.

## Features

`--no-default-features` is portable and compiles no window backend, Tokio runtime, image stack,
SVG stack, or diagnostics provider.

Feature ownership is explicit:

- `backend-winit` requires `renderer-skia`, because its software fallback and presentation
  path are Skia-based.
- `renderer-skia` remains portable and does not enable Win32.
- GL, Vulkan, and Metal features add only their Winit surface adapters and target dependencies.
- `notifications` and `tray` enable only portable APIs. `notifications-win32` and `tray-win32`
  add the Windows adapters without forcing a window backend.
- Accessibility, images, SVG, and diagnostics stay independently gated.
- `persistent-cache` adds the portable store contract and file-store implementation without
  enabling a renderer or platform backend.

The CI matrix checks portable no-default tests, standalone Winit,
default tests, and all-feature tests. Architecture tests enforce directory ownership, dependency
direction, real-module assembly, and the feature graph.
